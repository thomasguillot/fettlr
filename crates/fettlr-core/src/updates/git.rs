use std::path::Path;
use std::time::Duration;

use crate::cli::{CliOutput, CliRunner, ProcessCliRunner};
use crate::cli_records::CoreError;

pub trait GitProbing: Send + Sync {
    fn behind_count(&self, repo: &Path) -> Result<u32, CoreError>;
    fn pull_ff_only(&self, repo: &Path) -> Result<(), CoreError>;

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

    fn fetch_source(
        &self,
        url: &str,
        reference: Option<&str>,
        cache_repo: &Path,
    ) -> Result<String, CoreError> {
        std::fs::create_dir_all(cache_repo).map_err(|error| CoreError::Cli {
            command: "git init".to_string(),
            stderr: error.to_string(),
        })?;
        if !cache_repo.join("HEAD").exists() {
            self.run_ok(&["init", "--quiet", "--bare", "."], cache_repo)?;
        }
        self.run_ok(&["config", "remote.origin.url", url], cache_repo)?;
        self.run_ok(&["config", "remote.origin.promisor", "true"], cache_repo)?;
        self.run_ok(
            &["config", "remote.origin.partialclonefilter", "blob:none"],
            cache_repo,
        )?;

        let target = reference.unwrap_or("HEAD");
        let listing_args = ["ls-remote", "origin", target];
        let listing = self.run_ok(&listing_args, cache_repo)?;
        let commit = listing
            .stdout
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().next())
            .filter(|sha| is_commit_sha(sha))
            .ok_or_else(|| failed(&listing_args, listing.stdout.clone()))?
            .to_string();

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
        if folder.is_empty() {
            let root = format!("{commit}^{{tree}}");
            let output = self.run_ok(&["rev-parse", &root], cache_repo)?;
            return Ok(Some(output.stdout.trim().to_string()));
        }
        let output = self.run_ok(
            &[
                "ls-tree",
                "-d",
                "--format=%(objectname)",
                commit,
                "--",
                folder,
            ],
            cache_repo,
        )?;
        let sha = output.stdout.trim();
        Ok((!sha.is_empty()).then(|| sha.to_string()))
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
            &[&git_dir, &work_tree, "checkout", commit, "--", pathspec],
            cache_repo,
        )?;
        Ok(())
    }
}
