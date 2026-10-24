use std::path::Path;
use std::time::Duration;

use crate::cli::{CliOutput, CliRunner, ProcessCliRunner};
use crate::cli_records::CoreError;

pub trait GitProbing: Send + Sync {
    fn behind_count(&self, repo: &Path) -> Result<u32, CoreError>;
    fn pull_ff_only(&self, repo: &Path) -> Result<(), CoreError>;

    /// The 40-hex commit the remote's `reference` (or `HEAD`) points at, from `git ls-remote`
    /// alone: nothing is fetched. `workdir` is only the directory the command runs in.
    fn remote_head(
        &self,
        url: &str,
        reference: Option<&str>,
        workdir: &Path,
    ) -> Result<String, CoreError>;

    /// Returns the 40-hex commit the remote's `reference` (or `HEAD`) points at, with that
    /// commit's trees present in the blob-less bare `cache_repo`.
    fn fetch_source(
        &self,
        url: &str,
        reference: Option<&str>,
        cache_repo: &Path,
    ) -> Result<String, CoreError>;

    /// The folder's tree sha at `commit`; `None` when the path is missing or not a directory.
    /// An empty `folder` means the repository root.
    fn tree_sha(
        &self,
        cache_repo: &Path,
        commit: &str,
        folder: &str,
    ) -> Result<Option<String>, CoreError>;

    /// Writes the folder's files under `destination/<folder>`, or the whole tree under
    /// `destination` when `folder` is empty.
    fn export_folder(
        &self,
        cache_repo: &Path,
        commit: &str,
        folder: &str,
        destination: &Path,
    ) -> Result<(), CoreError>;
}

#[derive(Debug, Default)]
pub struct GitProbe;

impl GitProbe {
    const BINARY: &'static str = "/usr/bin/git";
    const TIMEOUT: Duration = Duration::from_secs(30);

    fn run(&self, args: &[&str], repo: &Path) -> Result<CliOutput, CoreError> {
        let owned: Vec<String> = HARDENING
            .iter()
            .chain(args)
            .map(|arg| (*arg).to_string())
            .collect();
        ProcessCliRunner.run(Path::new(Self::BINARY), &owned, Some(repo), Self::TIMEOUT)
    }

    /// The url may carry credentials, so a failure names the command with `<url>` in its place.
    fn set_origin_url(&self, url: &str, repo: &Path) -> Result<(), CoreError> {
        const SHOWN: &[&str] = &["config", "remote.origin.url", "<url>"];
        let output = self
            .run(&["config", "remote.origin.url", url], repo)
            .map_err(|error| match error {
                CoreError::Timeout { .. } => CoreError::Timeout {
                    command: format!("git {}", SHOWN.join(" ")),
                },
                other => other,
            })?;
        if output.exit_code != 0 {
            return Err(failed(SHOWN, output.stderr));
        }
        Ok(())
    }

    fn list_remote(&self, remote: &str, target: &str, workdir: &Path) -> Result<String, CoreError> {
        let args = ["ls-remote", "--end-of-options", remote, target];
        let listing = self.run_ok(&args, workdir)?;
        first_commit(&listing.stdout).ok_or_else(|| failed(&args, listing.stdout.clone()))
    }

    fn run_ok(&self, args: &[&str], repo: &Path) -> Result<CliOutput, CoreError> {
        let output = self.run(args, repo)?;
        if output.exit_code != 0 {
            return Err(failed(args, output.stderr));
        }
        Ok(output)
    }
}

const HARDENING: &[&str] = &[
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.hooksPath=/dev/null",
];

// Built from the arguments actually run, so an edit to one cannot leave the error naming a command
// that never ran.
fn failed(args: &[&str], stderr: String) -> CoreError {
    CoreError::Cli {
        command: format!("git {}", args.join(" ")),
        stderr,
    }
}

fn is_commit_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn first_commit(listing: &str) -> Option<String> {
    listing
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .filter(|sha| matches!(sha.len(), 40 | 64) && sha.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_string)
}

fn checked_remote(url: &str, reference: Option<&str>) -> Result<(), CoreError> {
    if !(url.starts_with("https://") || url.starts_with("file://")) {
        return Err(CoreError::Cli {
            command: "fettlr: unsupported source url".into(),
            stderr: "refusing a source url that is not https:// or file://".into(),
        });
    }
    if reference.is_some_and(|reference| reference.starts_with('-')) {
        return Err(CoreError::Cli {
            command: "fettlr: invalid reference".into(),
            stderr: "refusing a reference that starts with a dash".into(),
        });
    }
    Ok(())
}

const BEHIND_COUNT: &[&str] = &["rev-list", "--count", "HEAD..@{upstream}"];

impl GitProbing for GitProbe {
    fn behind_count(&self, repo: &Path) -> Result<u32, CoreError> {
        self.run_ok(&["fetch", "--quiet"], repo)?;

        let counted = self.run(BEHIND_COUNT, repo)?;
        if counted.exit_code != 0 {
            if counted.stderr.contains("no upstream configured") {
                return Err(CoreError::NoUpstream {
                    repo: repo.display().to_string(),
                });
            }
            return Err(failed(BEHIND_COUNT, counted.stderr));
        }

        counted
            .stdout
            .trim()
            .parse::<u32>()
            .map_err(|_| failed(BEHIND_COUNT, counted.stdout))
    }

    fn pull_ff_only(&self, repo: &Path) -> Result<(), CoreError> {
        self.run_ok(&["pull", "--ff-only", "--quiet"], repo)?;
        Ok(())
    }

    fn remote_head(
        &self,
        url: &str,
        reference: Option<&str>,
        workdir: &Path,
    ) -> Result<String, CoreError> {
        checked_remote(url, reference)?;
        let target = reference.unwrap_or("HEAD");
        let no_repo = format!("--git-dir={}", workdir.join("fettlr-no-git-dir").display());
        let shown = ["ls-remote", "--end-of-options", "<url>", target];
        let listing = self
            .run(
                &[&no_repo, "ls-remote", "--end-of-options", url, target],
                workdir,
            )
            .map_err(|error| match error {
                CoreError::Timeout { .. } => CoreError::Timeout {
                    command: format!("git {}", shown.join(" ")),
                },
                other => other,
            })?;
        if listing.exit_code != 0 {
            return Err(failed(&shown, listing.stderr));
        }
        first_commit(&listing.stdout).ok_or_else(|| failed(&shown, listing.stdout.clone()))
    }

    fn fetch_source(
        &self,
        url: &str,
        reference: Option<&str>,
        cache_repo: &Path,
    ) -> Result<String, CoreError> {
        checked_remote(url, reference)?;
        std::fs::create_dir_all(cache_repo)?;
        if !cache_repo.join("HEAD").exists() {
            self.run_ok(&["init", "--quiet", "--bare", "."], cache_repo)?;
        }
        self.set_origin_url(url, cache_repo)?;
        self.run_ok(&["config", "remote.origin.promisor", "true"], cache_repo)?;
        self.run_ok(
            &["config", "remote.origin.partialclonefilter", "blob:none"],
            cache_repo,
        )?;

        let target = reference.unwrap_or("HEAD");
        let commit = self.list_remote("origin", target, cache_repo)?;

        let fetched = self.run(
            &["rev-parse", "--verify", "--quiet", "FETCH_HEAD"],
            cache_repo,
        )?;
        if fetched.exit_code == 0 && fetched.stdout.trim() == commit {
            return Ok(commit);
        }

        self.run_ok(
            &[
                "fetch",
                "--quiet",
                "--depth=1",
                "--filter=blob:none",
                "--end-of-options",
                "origin",
                target,
            ],
            cache_repo,
        )?;
        let head = self.run_ok(&["rev-parse", "FETCH_HEAD"], cache_repo)?;
        Ok(head.stdout.trim().to_string())
    }

    fn tree_sha(
        &self,
        cache_repo: &Path,
        commit: &str,
        folder: &str,
    ) -> Result<Option<String>, CoreError> {
        let folder = folder.strip_suffix('/').unwrap_or(folder);
        if folder.is_empty() || folder == "." {
            let root = format!("{commit}^{{tree}}");
            let output = self.run_ok(&["rev-parse", &root], cache_repo)?;
            return Ok(Some(output.stdout.trim().to_string()));
        }
        let output = self.run_ok(
            &["--literal-pathspecs", "ls-tree", "-d", commit, "--", folder],
            cache_repo,
        )?;
        let mut lines = output.stdout.lines();
        let (Some(line), None) = (lines.next(), lines.next()) else {
            return Ok(None);
        };
        let Some((meta, path)) = line.split_once('\t') else {
            return Ok(None);
        };
        let fields: Vec<&str> = meta.split(' ').collect();
        Ok(match fields.as_slice() {
            [_, "tree", sha] if path == folder && is_commit_sha(sha) => Some((*sha).to_string()),
            _ => None,
        })
    }

    fn export_folder(
        &self,
        cache_repo: &Path,
        commit: &str,
        folder: &str,
        destination: &Path,
    ) -> Result<(), CoreError> {
        let git_dir = format!("--git-dir={}", cache_repo.display());
        let work_tree = format!("--work-tree={}", destination.display());
        let pathspec = if folder.is_empty() { "." } else { folder };
        self.run_ok(
            &[
                "--literal-pathspecs",
                &git_dir,
                &work_tree,
                "checkout",
                commit,
                "--",
                pathspec,
            ],
            cache_repo,
        )?;
        Ok(())
    }
}
