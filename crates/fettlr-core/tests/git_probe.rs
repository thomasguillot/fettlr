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

struct SourceFixture {
    _root: tempfile::TempDir,
    origin: PathBuf,
    cache: PathBuf,
    url: String,
}

impl SourceFixture {
    fn new() -> Self {
        let root = tempfile::TempDir::new().expect("create temp dir");
        let origin = root.path().join("origin");
        std::fs::create_dir_all(origin.join("skills/demo")).expect("create skill dir");
        run(
            &["-c", "init.defaultBranch=main", "init", "--quiet"],
            &origin,
        );
        run(&["config", "uploadpack.allowFilter", "true"], &origin);
        std::fs::write(origin.join("skills/demo/SKILL.md"), "demo skill").expect("write skill");
        std::fs::write(origin.join("other.txt"), "other").expect("write other");
        run(&["add", "."], &origin);
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
                "add skill",
            ],
            &origin,
        );
        let url = format!("file://{}", origin.display());
        let cache = root.path().join("cache");
        Self {
            _root: root,
            origin,
            cache,
            url,
        }
    }

    fn origin_rev(&self, spec: &str) -> String {
        let output = Command::new(GIT_BINARY)
            .args(["rev-parse", spec])
            .current_dir(&self.origin)
            .output()
            .expect("run git");
        assert!(output.status.success(), "rev-parse {spec} failed");
        String::from_utf8(output.stdout)
            .expect("utf8")
            .trim()
            .to_string()
    }
}

#[test]
fn fetch_source_returns_the_remote_head_and_fills_the_cache() {
    if !git_installed() {
        return;
    }
    let fixture = SourceFixture::new();

    let commit = GitProbe
        .fetch_source(&fixture.url, None, &fixture.cache)
        .expect("fetch source");

    assert_eq!(commit, fixture.origin_rev("HEAD"));
    assert!(fixture.cache.join("HEAD").exists());
}

#[test]
fn a_second_fetch_of_an_unchanged_remote_does_not_fetch_again() {
    if !git_installed() {
        return;
    }
    let fixture = SourceFixture::new();
    let fetch_head = fixture.cache.join("FETCH_HEAD");

    let first = GitProbe
        .fetch_source(&fixture.url, None, &fixture.cache)
        .expect("first fetch");
    let first_modified = std::fs::metadata(&fetch_head)
        .and_then(|meta| meta.modified())
        .expect("FETCH_HEAD after first fetch");
    std::thread::sleep(std::time::Duration::from_millis(50));
    let second = GitProbe
        .fetch_source(&fixture.url, None, &fixture.cache)
        .expect("second fetch");
    let second_modified = std::fs::metadata(&fetch_head)
        .and_then(|meta| meta.modified())
        .expect("FETCH_HEAD after second fetch");

    assert_eq!(first, second);
    assert_eq!(first_modified, second_modified);
}

#[test]
fn tree_sha_matches_the_origin_and_is_none_for_a_missing_or_file_path() {
    if !git_installed() {
        return;
    }
    let fixture = SourceFixture::new();
    let commit = GitProbe
        .fetch_source(&fixture.url, None, &fixture.cache)
        .expect("fetch source");

    assert_eq!(
        GitProbe
            .tree_sha(&fixture.cache, &commit, "skills/demo")
            .expect("tree sha"),
        Some(fixture.origin_rev("HEAD:skills/demo"))
    );
    assert_eq!(
        GitProbe
            .tree_sha(&fixture.cache, &commit, "nope")
            .expect("missing"),
        None
    );
    assert_eq!(
        GitProbe
            .tree_sha(&fixture.cache, &commit, "other.txt")
            .expect("file"),
        None
    );
    assert_eq!(
        GitProbe
            .tree_sha(&fixture.cache, &commit, "")
            .expect("root"),
        Some(fixture.origin_rev("HEAD^{tree}"))
    );
}

#[test]
fn export_folder_writes_only_that_folder() {
    if !git_installed() {
        return;
    }
    let fixture = SourceFixture::new();
    let commit = GitProbe
        .fetch_source(&fixture.url, None, &fixture.cache)
        .expect("fetch source");
    let destination = fixture._root.path().join("export");
    std::fs::create_dir_all(&destination).expect("create destination");

    GitProbe
        .export_folder(&fixture.cache, &commit, "skills/demo", &destination)
        .expect("export folder");

    assert_eq!(
        std::fs::read_to_string(destination.join("skills/demo/SKILL.md")).expect("read skill"),
        "demo skill"
    );
    assert!(!destination.join("other.txt").exists());
}

#[test]
fn fetch_source_reports_an_unreachable_remote() {
    if !git_installed() {
        return;
    }
    let root = tempfile::TempDir::new().expect("create temp dir");

    let error = GitProbe
        .fetch_source("file:///nonexistent", None, &root.path().join("cache"))
        .expect_err("unreachable");

    match error {
        CoreError::Cli { command, .. } => assert!(command.starts_with("git "), "{command}"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn a_fetch_after_the_remote_moves_fetches_the_new_head() {
    if !git_installed() {
        return;
    }
    let fixture = SourceFixture::new();
    let first = GitProbe
        .fetch_source(&fixture.url, None, &fixture.cache)
        .expect("first fetch");

    std::fs::write(fixture.origin.join("skills/demo/SKILL.md"), "changed").expect("write skill");
    run(&["add", "."], &fixture.origin);
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
            "change skill",
        ],
        &fixture.origin,
    );
    let second = GitProbe
        .fetch_source(&fixture.url, None, &fixture.cache)
        .expect("second fetch");

    assert_ne!(first, second);
    assert_eq!(second, fixture.origin_rev("HEAD"));
    assert_eq!(
        GitProbe
            .tree_sha(&fixture.cache, &second, "skills/demo")
            .expect("tree sha"),
        Some(fixture.origin_rev("HEAD:skills/demo"))
    );
}

#[test]
fn a_reference_starting_with_a_dash_is_refused() {
    if !git_installed() {
        return;
    }
    let fixture = SourceFixture::new();
    let marker_dir = tempfile::Builder::new()
        .prefix("fettlr-marker-")
        .tempdir()
        .expect("create marker dir");
    let marker = marker_dir.path().join("PWNED");
    let reference = format!("--upload-pack=touch$IFS{}", marker.display());
    run(&["branch", &format!("x/{reference}")], &fixture.origin);

    let result = GitProbe.fetch_source(&fixture.url, Some(&reference), &fixture.cache);

    match result {
        Err(CoreError::Cli { command, .. }) => assert_eq!(command, "fettlr: invalid reference"),
        other => panic!("unexpected result: {other:?}"),
    }
    assert!(!marker.exists());
    assert!(!fixture.cache.exists());
}

#[test]
fn a_source_url_that_is_not_https_or_file_is_refused_before_git_runs() {
    if !git_installed() {
        return;
    }
    let root = tempfile::TempDir::new().expect("create temp dir");
    let cache = root.path().join("cache");
    let marker = root.path().join("PWNED");
    for url in [
        format!("ext::sh -c touch% {}", marker.display()),
        "ssh://user:tok3n@github.com/a/b.git".to_string(),
        "git@github.com:a/b.git".to_string(),
        root.path().join("origin").display().to_string(),
    ] {
        let error = GitProbe
            .fetch_source(&url, None, &cache)
            .expect_err("refused");

        assert!(!error.to_string().contains(&url), "{error}");
        assert!(!error.to_string().contains("tok3n"), "{error}");
    }
    assert!(!marker.exists());
    assert!(!cache.exists());
}

#[test]
fn a_failure_to_record_the_source_url_never_names_it() {
    use std::os::unix::fs::PermissionsExt;

    if !git_installed() {
        return;
    }
    let root = tempfile::TempDir::new().expect("create temp dir");
    let cache = root.path().join("cache");
    std::fs::create_dir_all(&cache).expect("create cache");
    run(&["init", "--quiet", "--bare", "."], &cache);
    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o555)).expect("lock cache");
    let url = "https://user:tok3n@github.com/a/b.git";

    let result = GitProbe.fetch_source(url, None, &cache);

    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o755)).expect("unlock cache");
    let error = result.expect_err("config cannot be written");
    match &error {
        CoreError::Cli { command, .. } => {
            assert_eq!(command, "git config remote.origin.url <url>")
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert!(!error.to_string().contains("tok3n"), "{error}");
}

#[test]
fn tree_sha_treats_dot_as_the_root_and_ignores_a_trailing_slash() {
    if !git_installed() {
        return;
    }
    let fixture = SourceFixture::new();
    let commit = GitProbe
        .fetch_source(&fixture.url, None, &fixture.cache)
        .expect("fetch source");

    assert_eq!(
        GitProbe
            .tree_sha(&fixture.cache, &commit, ".")
            .expect("dot"),
        Some(fixture.origin_rev("HEAD^{tree}"))
    );
    assert_eq!(
        GitProbe
            .tree_sha(&fixture.cache, &commit, "skills/demo/")
            .expect("slash"),
        Some(fixture.origin_rev("HEAD:skills/demo"))
    );
}

#[test]
fn export_folder_treats_the_folder_literally() {
    if !git_installed() {
        return;
    }
    let fixture = SourceFixture::new();
    let commit = GitProbe
        .fetch_source(&fixture.url, None, &fixture.cache)
        .expect("fetch source");
    let destination = fixture._root.path().join("export");
    std::fs::create_dir_all(&destination).expect("create destination");

    let result = GitProbe.export_folder(&fixture.cache, &commit, "*", &destination);

    assert!(matches!(result, Err(CoreError::Cli { .. })));
    assert!(!destination.join("other.txt").exists());
    assert_eq!(
        GitProbe
            .tree_sha(&fixture.cache, &commit, "*")
            .expect("star"),
        None
    );
}

#[test]
fn remote_head_lists_the_head_without_fetching_and_refuses_a_dash_reference() {
    if !git_installed() {
        return;
    }
    let fixture = SourceFixture::new();
    let workdir = tempfile::TempDir::new().expect("create temp dir");

    let head = GitProbe
        .remote_head(&fixture.url, None, workdir.path())
        .expect("remote head");
    assert_eq!(head, fixture.origin_rev("HEAD"));
    assert!(!fixture.cache.exists());

    match GitProbe.remote_head(&fixture.url, Some("--x"), workdir.path()) {
        Err(CoreError::Cli { command, .. }) => assert_eq!(command, "fettlr: invalid reference"),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn remote_head_refuses_a_url_that_is_not_https_or_file() {
    let workdir = tempfile::TempDir::new().expect("create temp dir");
    for url in [
        "ssh://example.com/x",
        "ext::sh -c id",
        "-uhttps://x",
        "git@github.com:a/b.git",
    ] {
        match GitProbe.remote_head(url, None, workdir.path()) {
            Err(CoreError::Cli { command, .. }) => {
                assert_eq!(command, "fettlr: unsupported source url", "{url}")
            }
            other => panic!("unexpected result for {url}: {other:?}"),
        }
    }
}

#[test]
fn a_failed_remote_head_never_names_the_url_in_its_command() {
    if !git_installed() {
        return;
    }
    let workdir = tempfile::TempDir::new().expect("create temp dir");
    let url = "file:///nonexistent/secret-repo";
    match GitProbe.remote_head(url, None, workdir.path()) {
        Err(CoreError::Cli { command, .. }) => {
            assert!(!command.contains("secret-repo"), "{command}")
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn remote_head_ignores_a_repository_around_its_directory() {
    if !git_installed() {
        return;
    }
    let fixture = SourceFixture::new();
    let outer = tempfile::TempDir::new().expect("create temp dir");
    run(&["init", "--quiet"], outer.path());
    run(
        &[
            "config",
            "url.file:///nonexistent-redirect/.insteadOf",
            "file:///",
        ],
        outer.path(),
    );
    let inner = outer.path().join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    let head = GitProbe
        .remote_head(&fixture.url, None, &inner)
        .expect("remote head");
    assert_eq!(head, fixture.origin_rev("HEAD"));
}

#[test]
fn remote_head_returns_a_64_hex_head_from_a_sha_256_repository() {
    if !git_installed() {
        return;
    }
    let root = tempfile::TempDir::new().expect("create temp dir");
    let origin = root.path().join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    run(
        &[
            "-c",
            "init.defaultBranch=main",
            "init",
            "--quiet",
            "--object-format=sha256",
        ],
        &origin,
    );
    std::fs::write(origin.join("a.txt"), "a").unwrap();
    run(&["add", "."], &origin);
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
            "one",
        ],
        &origin,
    );
    let workdir = tempfile::TempDir::new().expect("create temp dir");
    let head = GitProbe
        .remote_head(
            &format!("file://{}", origin.display()),
            None,
            workdir.path(),
        )
        .expect("remote head");
    assert_eq!(head.len(), 64);
}
