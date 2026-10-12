use std::path::{Path, PathBuf};
use std::process::Command;

use fettlr_core::{CoreError, GitProbe, GitProbing};

const GIT_BINARY: &str = "/usr/bin/git";

fn git_installed() -> bool {
    Path::new(GIT_BINARY).is_file()
}

struct GitFixture {
    _root: tempfile::TempDir,
    origin: PathBuf,
    clone: PathBuf,
}

impl GitFixture {
    fn new() -> Self {
        let root = tempfile::TempDir::new().expect("create temp dir");
        let origin = root.path().join("origin");
        let clone = root.path().join("clone");
        std::fs::create_dir_all(&origin).expect("create origin dir");

        run(
            &["-c", "init.defaultBranch=main", "init", "--quiet"],
            &origin,
        );
        Self::commit(&origin, "one.txt");
        run(
            &[
                "clone",
                "--quiet",
                origin.to_str().expect("utf8 path"),
                clone.to_str().expect("utf8 path"),
            ],
            root.path(),
        );

        Self {
            _root: root,
            origin,
            clone,
        }
    }

    fn commit_to_origin(&self, file: &str) {
        Self::commit(&self.origin, file);
    }

    fn commit_to_clone(&self, file: &str) {
        Self::commit(&self.clone, file);
    }

    fn commit(dir: &Path, file: &str) {
        std::fs::write(dir.join(file), file).expect("write file");
        run(&["add", file], dir);
        run(
            &[
                "-c",
                "user.email=fettlr@example.test",
                "-c",
                "user.name=Fettlr",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                &format!("add {file}"),
            ],
            dir,
        );
    }
}

fn run(args: &[&str], cwd: &Path) {
    let output = Command::new(GIT_BINARY)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn behind_count_reports_zero_then_one_after_a_new_origin_commit() {
    if !git_installed() {
        return;
    }
    let fixture = GitFixture::new();
    let probe = GitProbe;

    assert_eq!(probe.behind_count(&fixture.clone).expect("behind count"), 0);

    fixture.commit_to_origin("two.txt");

    assert_eq!(probe.behind_count(&fixture.clone).expect("behind count"), 1);
}

#[test]
fn pull_ff_only_fast_forwards_the_clone() {
    if !git_installed() {
        return;
    }
    let fixture = GitFixture::new();
    let probe = GitProbe;

    fixture.commit_to_origin("two.txt");
    probe.pull_ff_only(&fixture.clone).expect("fast forward");

    assert!(fixture.clone.join("two.txt").exists());
}

#[test]
fn a_repository_without_an_upstream_reports_no_upstream() {
    if !git_installed() {
        return;
    }
    let fixture = GitFixture::new();
    let probe = GitProbe;

    let error = probe
        .behind_count(&fixture.origin)
        .expect_err("no upstream configured");
    assert!(matches!(error, CoreError::NoUpstream { .. }));
}

#[test]
fn a_fetch_that_cannot_reach_its_origin_reports_what_git_said() {
    if !git_installed() {
        return;
    }
    let fixture = GitFixture::new();
    std::fs::remove_dir_all(&fixture.origin).expect("remove origin");

    let error = GitProbe
        .behind_count(&fixture.clone)
        .expect_err("the origin is gone");

    match error {
        CoreError::Cli { command, stderr } => {
            assert_eq!(command, "git fetch --quiet");
            assert!(!stderr.is_empty());
        }
        other => panic!("expected a failed fetch, got {other:?}"),
    }
}

#[test]
fn a_pull_that_cannot_fast_forward_reports_what_git_said() {
    if !git_installed() {
        return;
    }
    let fixture = GitFixture::new();
    fixture.commit_to_origin("two.txt");
    fixture.commit_to_clone("three.txt");

    let error = GitProbe
        .pull_ff_only(&fixture.clone)
        .expect_err("the histories have diverged");

    match error {
        CoreError::Cli { command, stderr } => {
            assert_eq!(command, "git pull --ff-only --quiet");
            assert!(!stderr.is_empty());
        }
        other => panic!("expected a refused fast-forward, got {other:?}"),
    }
}
