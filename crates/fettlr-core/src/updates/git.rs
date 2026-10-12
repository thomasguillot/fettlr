use std::path::Path;
use std::time::Duration;

use crate::cli::{CliOutput, CliRunner, ProcessCliRunner};
use crate::cli_records::CoreError;

pub trait GitProbing: Send + Sync {
    fn behind_count(&self, repo: &Path) -> Result<u32, CoreError>;
    fn pull_ff_only(&self, repo: &Path) -> Result<(), CoreError>;
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
}
