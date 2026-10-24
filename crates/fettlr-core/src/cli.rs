use std::ffi::OsString;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::cli_records::{
    CoreError, MarketplaceRecord, McpListing, PluginRecord, parse_marketplace_list, parse_mcp_list,
    parse_plugin_catalogue, parse_plugin_list,
};
use crate::models::Scope;
use crate::paths::Paths;

const HOMEBREW_CLAUDE: &str = "/opt/homebrew/bin/claude";
const USR_LOCAL_CLAUDE: &str = "/usr/local/bin/claude";
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const TERM_GRACE_PERIOD: Duration = Duration::from_secs(2);
/// How long the pipes are given after the child has already exited. Whatever it wrote is buffered
/// by then, so this only ever elapses when something else is still holding the write end open.
const READER_GRACE_PERIOD: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub trait CliRunner: Send + Sync {
    fn run(
        &self,
        binary: &Path,
        args: &[String],
        cwd: Option<&Path>,
        timeout: Duration,
    ) -> Result<CliOutput, CoreError>;
}

#[derive(Debug, Default)]
pub struct ProcessCliRunner;

impl CliRunner for ProcessCliRunner {
    fn run(
        &self,
        binary: &Path,
        args: &[String],
        cwd: Option<&Path>,
        timeout: Duration,
    ) -> Result<CliOutput, CoreError> {
        let command = format!("{} {}", binary.display(), args.join(" "));

        if let Some(dir) = cwd
            && !dir.is_dir()
        {
            return Err(CoreError::MissingProjectDirectory {
                path: dir.display().to_string(),
            });
        }

        let mut command_builder = Command::new(binary);
        if binary.file_name().is_some_and(|name| name == "claude") {
            command_builder.env(
                "PATH",
                compose_path(
                    binary,
                    std::fs::canonicalize(binary).ok().as_deref(),
                    std::env::var_os("PATH"),
                ),
            );
        }
        command_builder
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = cwd {
            command_builder.current_dir(dir);
        }
        // claude is a launcher that spawns children (git, node); putting it in its own session
        // means a timeout can signal the whole group instead of leaving descendants running and
        // holding the pipes open.
        unsafe {
            command_builder.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }

        let mut child = command_builder
            .spawn()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => {
                    CoreError::BinaryNotFound
                }
                _ => CoreError::Io(error),
            })?;
        let mut stdout_pipe = child.stdout.take().expect("stdout was piped");
        let mut stderr_pipe = child.stderr.take().expect("stderr was piped");

        // Channels rather than join handles: a join cannot be given a deadline, and read_to_end
        // returns only once the last holder of the write end closes it, which is not necessarily
        // the child. `claude` and `git` both spawn descendants that inherit these pipes.
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let stdout_reader = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout_pipe.read_to_end(&mut buf);
            let _ = stdout_tx.send(buf);
        });
        let (stderr_tx, stderr_rx) = mpsc::channel();
        let stderr_reader = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stderr_pipe.read_to_end(&mut buf);
            let _ = stderr_tx.send(buf);
        });

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        break None;
                    }
                    thread::sleep(POLL_INTERVAL);
                }
                Err(_) => break None,
            }
        };

        match status {
            Some(status) => {
                // A descendant outliving its parent while holding the pipes would otherwise block
                // here with no bound at all, past the timeout the caller asked for. Reporting a
                // timeout is the honest answer: the output is not all here, and reading a short
                // one as the whole answer is how an empty list gets mistaken for an empty machine.
                let reader_deadline = deadline.max(Instant::now() + READER_GRACE_PERIOD);
                let (Some(stdout), Some(stderr)) = (
                    collected(&stdout_rx, reader_deadline),
                    collected(&stderr_rx, reader_deadline),
                ) else {
                    drop_readers_in_background(stdout_reader, stderr_reader);
                    return Err(CoreError::Timeout { command });
                };
                Ok(CliOutput {
                    stdout: String::from_utf8_lossy(&stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&stderr).into_owned(),
                    exit_code: status.code().unwrap_or(-1),
                })
            }
            None => {
                signal_group(child.id(), libc::SIGTERM);
                escalate_to_kill_in_background(child, stdout_reader, stderr_reader);
                Err(CoreError::Timeout { command })
            }
        }
    }
}

/// The directories a script launcher needs: the binary's own folder first (so `#!/usr/bin/env
/// node` finds the node installed beside it), then the folder of its symlink target, then whatever
/// the app inherited, which from Finder is only the system directories.
fn compose_path(binary: &Path, canonical: Option<&Path>, inherited: Option<OsString>) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |dir: Option<&Path>| {
        if let Some(dir) = dir
            && !dir.as_os_str().is_empty()
            && !dirs.iter().any(|existing| existing == dir)
        {
            dirs.push(dir.to_path_buf());
        }
    };
    push(binary.parent());
    push(canonical.and_then(Path::parent));
    let mut composed = std::env::join_paths(dirs).unwrap_or_default();
    if let Some(inherited) = inherited.filter(|value| !value.is_empty()) {
        if !composed.is_empty() {
            composed.push(":");
        }
        composed.push(inherited);
    }
    composed
}

fn collected(pipe: &mpsc::Receiver<Vec<u8>>, deadline: Instant) -> Option<Vec<u8>> {
    pipe.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
}

/// The reader threads own only their pipe and their buffer, so leaving them to finish whenever the
/// descendant closes it costs nothing the caller has to wait for.
fn drop_readers_in_background(
    stdout_reader: thread::JoinHandle<()>,
    stderr_reader: thread::JoinHandle<()>,
) {
    thread::spawn(move || {
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
    });
}

fn escalate_to_kill_in_background(
    mut child: Child,
    stdout_reader: thread::JoinHandle<()>,
    stderr_reader: thread::JoinHandle<()>,
) {
    thread::spawn(move || {
        let pid = child.id();
        let grace_deadline = Instant::now() + TERM_GRACE_PERIOD;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= grace_deadline {
                        signal_group(pid, libc::SIGKILL);
                        let _ = child.wait();
                        break;
                    }
                    thread::sleep(POLL_INTERVAL);
                }
                Err(_) => break,
            }
        }
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
    });
}

// Only signal the process group when the child is actually its own group leader (set up via
// setsid in pre_exec); otherwise a negative pid could hit an unrelated group.
fn signal_group(pid: u32, signal: libc::c_int) {
    let pid = pid as libc::pid_t;
    unsafe {
        if libc::getpgid(pid) == pid {
            libc::kill(-pid, signal);
        } else {
            libc::kill(pid, signal);
        }
    }
}

pub struct ClaudeCli {
    runner: Arc<dyn CliRunner>,
    paths: Paths,
    cached_binary: Mutex<Option<PathBuf>>,
}

impl ClaudeCli {
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
    const MARKETPLACE_TIMEOUT: Duration = Duration::from_secs(300);
    /// `claude mcp list` health-checks every server before it answers, which means network calls
    /// and spawning every stdio server. It is the slowest thing Fettlr runs by two orders of
    /// magnitude, which is why nothing calls it on a refresh.
    const MCP_LIST_TIMEOUT: Duration = Duration::from_secs(120);

    pub fn new(runner: Arc<dyn CliRunner>, paths: &Paths) -> Self {
        Self {
            runner,
            paths: paths.clone(),
            cached_binary: Mutex::new(None),
        }
    }

    pub fn binary(&self) -> Result<PathBuf, CoreError> {
        {
            let cached = self.cached_binary.lock().expect("lock poisoned");
            if let Some(path) = cached.as_ref() {
                return Ok(path.clone());
            }
        }
        let found = self.locate_binary()?;
        *self.cached_binary.lock().expect("lock poisoned") = Some(found.clone());
        Ok(found)
    }

    // A cached path that no longer spawns would otherwise be retried until the app restarts, so
    // moving or reinstalling `claude` while the window is open could not be recovered from.
    fn forget_binary(&self) {
        *self.cached_binary.lock().expect("lock poisoned") = None;
    }

    fn locate_binary(&self) -> Result<PathBuf, CoreError> {
        let mut candidates = Vec::new();
        if let Some(over) = &self.paths.binary_override {
            candidates.push(over.clone());
        }
        candidates.push(self.paths.claude_dir.join("local/claude"));
        candidates.push(PathBuf::from(HOMEBREW_CLAUDE));
        candidates.push(PathBuf::from(USR_LOCAL_CLAUDE));
        if let Some(home) = self.paths.claude_dir.parent() {
            candidates.push(home.join(".local/bin/claude"));
        }

        for candidate in candidates {
            if is_executable(&candidate) {
                return Ok(candidate);
            }
        }

        self.probe_login_shell().ok_or(CoreError::BinaryNotFound)
    }

    fn probe_login_shell(&self) -> Option<PathBuf> {
        let args = vec!["-lc".to_string(), "command -v claude".to_string()];
        let output = self
            .runner
            .run(Path::new("/bin/zsh"), &args, None, Self::DEFAULT_TIMEOUT)
            .ok()?;
        if output.exit_code != 0 {
            return None;
        }
        parse_login_shell_output(&output.stdout)
    }

    /// Installed plugins only. Without `--available` the output is a few kilobytes; the catalogue
    /// is megabytes and comes from `plugin_catalogue()`.
    pub fn list_plugins(&self) -> Result<Vec<PluginRecord>, CoreError> {
        let output = self.run(&args(&["plugin", "list", "--json"]), None)?;
        parse_plugin_list(&output.stdout)
    }

    /// The catalogue Claude Code offers for install. It leaves out plugins already installed at
    /// user scope or enabled in the current project, so it feeds Browse only and is not an update
    /// source.
    pub fn plugin_catalogue(&self) -> Result<Vec<PluginRecord>, CoreError> {
        let args = args(&["plugin", "list", "--json", "--available"]);
        let output = self.run(&args, None)?;
        parse_plugin_catalogue(&output.stdout)
    }

    pub fn list_marketplaces(&self) -> Result<Vec<MarketplaceRecord>, CoreError> {
        let args = args(&["plugin", "marketplace", "list", "--json"]);
        let output = self.run(&args, None)?;
        parse_marketplace_list(&output.stdout)
    }

    pub fn plugin_details(&self, id: &str) -> Result<String, CoreError> {
        let args = args(&["plugin", "details", id]);
        Ok(self.run(&args, None)?.stdout)
    }

    pub fn mcp_get(&self, name: &str, project_path: Option<&str>) -> Result<String, CoreError> {
        let args = args(&["mcp", "get", name]);
        Ok(self.run(&args, project_path.map(Path::new))?.stdout)
    }

    pub fn install_plugin(&self, plugin_id: &str, scope: &Scope) -> Result<(), CoreError> {
        let args = args(&[
            "plugin",
            "install",
            plugin_id,
            "-s",
            scope_flag(scope)?,
            "-y",
        ]);
        self.run(&args, scope.project_path().map(Path::new))?;
        Ok(())
    }

    pub fn uninstall_plugin(&self, plugin_id: &str, scope: &Scope) -> Result<(), CoreError> {
        let args = args(&["plugin", "uninstall", plugin_id, "-s", scope_flag(scope)?]);
        self.run(&args, scope.project_path().map(Path::new))?;
        Ok(())
    }

    pub fn set_plugin_enabled(
        &self,
        plugin_id: &str,
        enabled: bool,
        scope: &Scope,
    ) -> Result<(), CoreError> {
        let verb = if enabled { "enable" } else { "disable" };
        let args = args(&["plugin", verb, plugin_id, "-s", scope_flag(scope)?]);
        self.run(&args, scope.project_path().map(Path::new))?;
        Ok(())
    }

    pub fn update_plugin(&self, plugin_id: &str, scope: &Scope) -> Result<(), CoreError> {
        let args = args(&[
            "plugin",
            "update",
            plugin_id,
            "-s",
            scope_flag(scope)?,
            "-y",
        ]);
        self.run(&args, scope.project_path().map(Path::new))?;
        Ok(())
    }

    pub fn add_marketplace(&self, source: &str) -> Result<(), CoreError> {
        let args = args(&["plugin", "marketplace", "add", source]);
        self.run(&args, None)?;
        Ok(())
    }

    pub fn remove_marketplace(&self, name: &str) -> Result<(), CoreError> {
        let args = args(&["plugin", "marketplace", "remove", name]);
        self.run(&args, None)?;
        Ok(())
    }

    pub fn update_marketplace(&self, name: Option<&str>) -> Result<(), CoreError> {
        let mut args = args(&["plugin", "marketplace", "update"]);
        if let Some(name) = name {
            args.push(name.to_string());
        }
        self.run_with_timeout(&args, None, Self::MARKETPLACE_TIMEOUT)?;
        Ok(())
    }

    pub fn mcp_add_json(
        &self,
        name: &str,
        config_json: &str,
        scope: &Scope,
        local: bool,
    ) -> Result<(), CoreError> {
        let scope_value = if local { "local" } else { scope_flag(scope)? };
        // One shape for both: the real arguments and the logged ones used to be written out side
        // by side and kept in step by hand, where a flag added to one and not the other would have
        // put the config, secrets included, into the logged command line.
        let line = |config: &str| args(&["mcp", "add-json", name, config, "-s", scope_value]);
        self.run_redacted(
            &line(config_json),
            &line("<config redacted>"),
            scope.project_path().map(Path::new),
        )?;
        Ok(())
    }

    pub fn mcp_list(&self) -> Result<Vec<McpListing>, CoreError> {
        let args = args(&["mcp", "list"]);
        let output = self.run_with_timeout(&args, None, Self::MCP_LIST_TIMEOUT)?;
        parse_mcp_list(&output.stdout)
    }

    pub fn mcp_remove(&self, name: &str, scope: &Scope, local: bool) -> Result<(), CoreError> {
        let scope_value = if local { "local" } else { scope_flag(scope)? };
        let args = args(&["mcp", "remove", name, "-s", scope_value]);
        self.run(&args, scope.project_path().map(Path::new))?;
        Ok(())
    }

    fn run(&self, args: &[String], cwd: Option<&Path>) -> Result<CliOutput, CoreError> {
        self.run_with_timeout(args, cwd, Self::DEFAULT_TIMEOUT)
    }

    fn run_with_timeout(
        &self,
        args: &[String],
        cwd: Option<&Path>,
        timeout: Duration,
    ) -> Result<CliOutput, CoreError> {
        let command = command_line(args);
        log::debug!("{command}");
        self.execute(args, cwd, timeout, command)
    }

    fn run_redacted(
        &self,
        args: &[String],
        redacted_args: &[String],
        cwd: Option<&Path>,
    ) -> Result<CliOutput, CoreError> {
        let command = command_line(redacted_args);
        log::debug!("{command}");
        self.execute(args, cwd, Self::DEFAULT_TIMEOUT, command)
    }

    fn execute(
        &self,
        args: &[String],
        cwd: Option<&Path>,
        timeout: Duration,
        command: String,
    ) -> Result<CliOutput, CoreError> {
        let binary = self.binary()?;
        match self.runner.run(&binary, args, cwd, timeout) {
            Ok(output) if output.exit_code == 0 => Ok(output),
            // Not every tool diagnoses on stderr. Taking stdout when stderr is empty is the
            // difference between an error the reader can act on and a dialog with nothing in it.
            Ok(output) => Err(CoreError::Cli {
                command,
                stderr: if output.stderr.trim().is_empty() {
                    output.stdout
                } else {
                    output.stderr
                },
            }),
            Err(CoreError::Timeout { .. }) => Err(CoreError::Timeout { command }),
            Err(CoreError::Cli { stderr, .. }) => Err(CoreError::Cli { command, stderr }),
            Err(CoreError::BinaryNotFound) => {
                self.forget_binary();
                Err(CoreError::BinaryNotFound)
            }
            Err(other) => Err(other),
        }
    }
}

fn scope_flag(scope: &Scope) -> Result<&'static str, CoreError> {
    scope
        .cli_value()
        .ok_or_else(|| CoreError::UnsupportedScope {
            scope: match scope {
                Scope::Other { name } => name.clone(),
                _ => scope.key(),
            },
        })
}

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

fn command_line(args: &[String]) -> String {
    std::iter::once("claude".to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn parse_login_shell_output(stdout: &str) -> Option<PathBuf> {
    let first_line = stdout.lines().next()?.trim();
    first_line
        .starts_with('/')
        .then(|| PathBuf::from(first_line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_prefixes_claude() {
        assert_eq!(
            command_line(&["plugin".to_string(), "list".to_string()]),
            "claude plugin list"
        );
    }

    #[test]
    fn compose_path_puts_the_binary_dir_then_the_target_dir_then_the_inherited_path() {
        let composed = compose_path(
            Path::new("/opt/homebrew/bin/claude"),
            Some(Path::new(
                "/opt/homebrew/lib/node_modules/claude/bin/claude.js",
            )),
            Some(OsString::from("/usr/bin:/bin")),
        );
        assert_eq!(
            composed,
            OsString::from(
                "/opt/homebrew/bin:/opt/homebrew/lib/node_modules/claude/bin:/usr/bin:/bin"
            )
        );
    }

    #[test]
    fn compose_path_skips_a_duplicate_target_dir_and_a_missing_inherited_path() {
        let composed = compose_path(
            Path::new("/usr/local/bin/claude"),
            Some(Path::new("/usr/local/bin/claude")),
            None,
        );
        assert_eq!(composed, OsString::from("/usr/local/bin"));
    }

    #[test]
    fn command_line_with_no_args_is_just_claude() {
        assert_eq!(command_line(&[]), "claude");
    }

    #[test]
    fn parse_login_shell_output_takes_the_first_line_when_absolute() {
        assert_eq!(
            parse_login_shell_output("/opt/homebrew/bin/claude\n"),
            Some(PathBuf::from("/opt/homebrew/bin/claude"))
        );
    }

    #[test]
    fn parse_login_shell_output_rejects_relative_paths() {
        assert_eq!(parse_login_shell_output("claude: aliased to foo\n"), None);
    }

    #[test]
    fn parse_login_shell_output_rejects_empty_output() {
        assert_eq!(parse_login_shell_output(""), None);
    }
}
