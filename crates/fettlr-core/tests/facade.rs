use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fettlr_core::testing::{FakeGitProbe, FakeRegistryClient, FakeTrasher, TempTree};
use fettlr_core::{
    CliOutput, CliRunner, CoreError, Fettlr, Scope, UpdateItemKind, UpdateProgress, UpdateStage,
    UpdateState, Watcher,
};
use serde_json::json;

const PLUGIN_LIST: &str = r#"[
  {
    "id": "widgets@acme",
    "version": "1.0.0",
    "scope": "user",
    "enabled": true,
    "installPath": "/fake/home/.claude/plugins/cache/acme/widgets/1.0.0"
  }
]"#;

const PLUGIN_LIST_AVAILABLE: &str = include_str!("fixtures/plugin-list-available.json");

const MARKETPLACE_LIST: &str = r#"[
  {
    "name": "acme",
    "source": "github",
    "repo": "acme/marketplace",
    "installLocation": "/fake/marketplaces/acme"
  }
]"#;

fn write_executable(path: &Path) {
    std::fs::create_dir_all(path.parent().expect("has parent")).expect("create dirs");
    std::fs::write(path, "#!/bin/sh\necho fake-claude\n").expect("write file");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod");
}

fn fake_binary_path() -> PathBuf {
    let path = std::env::temp_dir().join("fettlr-core-facade-tests-fake-claude");
    write_executable(&path);
    path
}

fn homebrew_or_usr_local_claude_present() -> bool {
    Path::new("/opt/homebrew/bin/claude").exists() || Path::new("/usr/local/bin/claude").exists()
}

#[derive(Default)]
struct ScriptedState {
    plugin_list: String,
    marketplace_list: String,
    fail_prefixes: Vec<Vec<String>>,
    binary_not_found_prefixes: Vec<Vec<String>>,
    calls: Vec<Vec<String>>,
}

/// A `CliRunner` that dispatches by argv, since `refresh()` and `check_updates()` issue several
/// different `claude` commands through the one injected runner. Each list can be reprogrammed
/// mid-test to simulate the CLI's own state changing between two reads.
struct ScriptedCliRunner {
    state: Mutex<ScriptedState>,
    /// Held outside `state`: a paused call must not keep the lock the call interleaving with it
    /// needs. `None` unless a test arms it.
    gate: Mutex<Option<Gate>>,
}

struct Gate {
    prefix: Vec<String>,
    entered: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

impl ScriptedCliRunner {
    fn new() -> Self {
        Self {
            state: Mutex::new(ScriptedState {
                plugin_list: "[]".to_string(),
                marketplace_list: "[]".to_string(),
                fail_prefixes: Vec::new(),
                binary_not_found_prefixes: Vec::new(),
                calls: Vec::new(),
            }),
            gate: Mutex::new(None),
        }
    }

    /// Pauses the next call whose argv starts with `prefix` until the returned handle is released,
    /// so a test can run something else while that call is in flight.
    fn pause_when_args_start_with(
        &self,
        prefix: &[&str],
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *self.gate.lock().expect("lock poisoned") = Some(Gate {
            prefix: prefix.iter().map(|arg| arg.to_string()).collect(),
            entered: entered_tx,
            release: release_rx,
        });
        (entered_rx, release_tx)
    }

    fn set_plugin_list(&self, json: &str) {
        self.state.lock().expect("lock poisoned").plugin_list = json.to_string();
    }

    fn set_marketplace_list(&self, json: &str) {
        self.state.lock().expect("lock poisoned").marketplace_list = json.to_string();
    }

    fn fail_when_args_start_with(&self, prefix: &[&str]) {
        self.state
            .lock()
            .expect("lock poisoned")
            .fail_prefixes
            .push(prefix.iter().map(|arg| arg.to_string()).collect());
    }

    fn fail_with_binary_not_found_when_args_start_with(&self, prefix: &[&str]) {
        self.state
            .lock()
            .expect("lock poisoned")
            .binary_not_found_prefixes
            .push(prefix.iter().map(|arg| arg.to_string()).collect());
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.state.lock().expect("lock poisoned").calls.clone()
    }
}

fn installed_only(listing: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(listing) {
        Ok(serde_json::Value::Object(mut object)) => {
            object.remove("available");
            serde_json::Value::Object(object).to_string()
        }
        _ => listing.to_string(),
    }
}

impl CliRunner for ScriptedCliRunner {
    fn run(
        &self,
        _binary: &Path,
        args: &[String],
        _cwd: Option<&Path>,
        _timeout: Duration,
    ) -> Result<CliOutput, CoreError> {
        let mut state = self.state.lock().expect("lock poisoned");
        state.calls.push(args.to_vec());

        if state
            .binary_not_found_prefixes
            .iter()
            .any(|prefix| args.starts_with(prefix.as_slice()))
        {
            return Err(CoreError::BinaryNotFound);
        }

        if state
            .fail_prefixes
            .iter()
            .any(|prefix| args.starts_with(prefix.as_slice()))
        {
            return Ok(CliOutput {
                stdout: String::new(),
                stderr: "boom".to_string(),
                exit_code: 1,
            });
        }

        let stdout = if args.starts_with(&[
            "plugin".to_string(),
            "list".to_string(),
            "--json".to_string(),
        ]) {
            if args.iter().any(|arg| arg == "--available") {
                state.plugin_list.clone()
            } else {
                installed_only(&state.plugin_list)
            }
        } else if args.starts_with(&[
            "plugin".to_string(),
            "marketplace".to_string(),
            "list".to_string(),
            "--json".to_string(),
        ]) {
            state.marketplace_list.clone()
        } else {
            String::new()
        };

        drop(state);

        let gated = {
            let mut gate = self.gate.lock().expect("lock poisoned");
            match gate.as_ref() {
                Some(armed) if args.starts_with(armed.prefix.as_slice()) => gate.take(),
                _ => None,
            }
        };
        if let Some(gate) = gated {
            let _ = gate.entered.send(());
            let _ = gate.release.recv();
        }

        Ok(CliOutput {
            stdout,
            stderr: String::new(),
            exit_code: 0,
        })
    }
}

fn fettlr_with(runner: Arc<ScriptedCliRunner>, tree: &TempTree) -> Fettlr {
    let mut paths = tree.paths.clone();
    paths.binary_override = Some(fake_binary_path());
    Fettlr::new(
        paths,
        runner,
        Arc::new(FakeGitProbe::new()),
        Arc::new(FakeRegistryClient::new()),
        Arc::new(FakeTrasher::new()),
    )
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn account_and_plugin_copy(tree: &TempTree) -> (PathBuf, Arc<ScriptedCliRunner>) {
    let uploaded = json!([{
        "skillId": "skill_x",
        "name": "impeccable",
        "source": "plugin",
        "updatedAt": "2026-10-24T12:57:30Z"
    }]);
    tree.write_account_manifest("b1", 1, uploaded);
    let account = tree.make_account_skill(
        "b1",
        "impeccable",
        "---\nname: impeccable\ndescription: d\n---\n\nold body\n",
    );
    tree.make_skill(
        "impeccable",
        &tree.plugin_skills_dir("impeccable"),
        Some("---\nname: impeccable\ndescription: d\nversion: 4.7.1\n---\n\nnew body\n"),
        false,
    );
    let install_path = tree.plugin_install_dir("impeccable").display().to_string();
    let runner = Arc::new(ScriptedCliRunner::new());
    runner.set_plugin_list(&format!(
        r#"[{{"id": "impeccable@impeccable", "version": "4.7.1", "scope": "user", "enabled": true, "installPath": "{install_path}"}}]"#
    ));
    (account, runner)
}

mod refresh {
    use super::*;

    #[test]
    fn refresh_flags_an_account_skill_a_newer_plugin_copy_supersedes() {
        let tree = TempTree::new();
        let (_, runner) = account_and_plugin_copy(&tree);
        let fettlr = fettlr_with(runner, &tree);

        let snapshot = fettlr.refresh().expect("refresh");

        assert_eq!(
            snapshot.account_skills_behind,
            vec![fettlr_core::AccountSkillBehind {
                account_id: "account:impeccable".into(),
                local_id: "plugin:impeccable@impeccable/impeccable".into(),
                local_version: Some("4.7.1".into()),
            }]
        );
    }

    #[test]
    fn assembles_a_snapshot_from_every_source() {
        let tree = TempTree::new();
        tree.make_skill(
            "demo",
            &tree.user_skills_dir(),
            Some(&TempTree::skill_markdown("demo", "A demo skill")),
            false,
        );
        let project = tree.make_project("example-project");
        tree.write_claude_json(json!({
            "mcpServers": {
                "context7": {"command": "npx", "args": ["-y", "ctx"]}
            },
            "projects": {
                project.display().to_string(): {}
            }
        }));

        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_plugin_list(PLUGIN_LIST);
        runner.set_marketplace_list(MARKETPLACE_LIST);
        let fettlr = fettlr_with(runner, &tree);

        let snapshot = fettlr.refresh().expect("refresh succeeds");

        assert!(snapshot.cli_available);
        assert_eq!(snapshot.skills.len(), 1);
        assert_eq!(snapshot.skills[0].name, "demo");
        assert_eq!(snapshot.plugins.len(), 1);
        assert_eq!(snapshot.plugins[0].plugin_id, "widgets@acme");
        assert_eq!(snapshot.marketplaces.len(), 1);
        assert_eq!(snapshot.marketplaces[0].name, "acme");
        assert!(snapshot.mcp_servers.iter().any(|s| s.name == "context7"));
        assert_eq!(snapshot.projects.len(), 1);
        assert!(snapshot.last_refreshed.is_some());
        assert!(snapshot.available_plugins.is_empty());
        assert!(snapshot.update_report.is_none());
    }

    #[test]
    fn refresh_lists_account_skills_beside_user_skills() {
        let tree = TempTree::new();
        tree.make_skill(
            "demo",
            &tree.user_skills_dir(),
            Some(&TempTree::skill_markdown("demo", "A demo skill")),
            false,
        );
        tree.write_account_manifest(
            "b1",
            1,
            json!([{"name": "impeccable", "source": "plugin", "updatedAt": "2026-10-24T12:57:30Z"}]),
        );
        tree.make_account_skill(
            "b1",
            "impeccable",
            &TempTree::skill_markdown("impeccable", "Design skill"),
        );
        let fettlr = fettlr_with(Arc::new(ScriptedCliRunner::new()), &tree);

        let snapshot = fettlr.refresh().expect("refresh succeeds");

        let mut ids: Vec<_> = snapshot.skills.iter().map(|s| s.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["account:impeccable", "user/demo"]);
    }

    #[test]
    fn merges_plugin_provided_skills_into_the_skills_list() {
        let tree = TempTree::new();
        tree.make_skill(
            "helper",
            &tree.plugin_skills_dir("acme-widgets"),
            Some(&TempTree::skill_markdown(
                "helper",
                "Ships with the plugin.",
            )),
            false,
        );
        let install_path = tree
            .plugin_install_dir("acme-widgets")
            .display()
            .to_string();
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_plugin_list(&format!(
            r#"[
  {{
    "id": "widgets@acme",
    "version": "1.0.0",
    "scope": "user",
    "enabled": true,
    "installPath": "{install_path}"
  }}
]"#
        ));
        let fettlr = fettlr_with(runner, &tree);

        let snapshot = fettlr.refresh().expect("refresh succeeds");

        assert!(
            snapshot
                .skills
                .iter()
                .any(|skill| skill.id == "plugin:widgets@acme/helper")
        );
    }

    #[test]
    fn when_the_binary_is_missing_file_data_still_shows_and_cli_lists_are_empty() {
        if homebrew_or_usr_local_claude_present() {
            eprintln!("skipping: a real claude binary is present on this machine");
            return;
        }

        let tree = TempTree::new();
        tree.make_skill(
            "demo",
            &tree.user_skills_dir(),
            Some(&TempTree::skill_markdown("demo", "A demo skill")),
            false,
        );
        let runner = Arc::new(ScriptedCliRunner::new());
        let fettlr = Fettlr::new(
            tree.paths.clone(),
            runner,
            Arc::new(FakeGitProbe::new()),
            Arc::new(FakeRegistryClient::new()),
            Arc::new(FakeTrasher::new()),
        );

        let snapshot = fettlr.refresh().expect("refresh does not fail");

        assert!(!snapshot.cli_available);
        assert!(snapshot.plugins.is_empty());
        assert!(snapshot.marketplaces.is_empty());
        assert_eq!(snapshot.skills.len(), 1);
    }
}

mod checking_for_updates {
    use super::*;

    #[test]
    fn a_binary_that_disappears_flips_cli_available_instead_of_erroring() {
        let tree = TempTree::new();
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_plugin_list(PLUGIN_LIST);
        runner.set_marketplace_list(MARKETPLACE_LIST);
        let fettlr = fettlr_with(runner.clone(), &tree);

        let baseline = fettlr.refresh().expect("baseline refresh succeeds");
        assert!(baseline.cli_available);

        runner.fail_with_binary_not_found_when_args_start_with(&[
            "plugin",
            "list",
            "--json",
            "--available",
        ]);

        let checked = fettlr
            .check_updates()
            .expect("check_updates does not error on a missing binary");

        assert!(!checked.cli_available);
        assert!(checked.available_plugins.is_empty());
    }

    #[test]
    fn check_updates_judges_installed_plugins_against_the_marketplace_on_disk() {
        let tree = TempTree::new();
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_plugin_list(PLUGIN_LIST_AVAILABLE);
        let dir = tree.write_marketplace(
            "accesslint",
            json!({"source": "github", "repo": "accesslint/accesslint"}),
            json!([{"name": "accesslint", "version": "0.10.2", "source": "./plugins/accesslint"}]),
        );
        tree.write_plugin_manifest(&dir, "./plugins/accesslint", Some("0.10.3"));
        let fettlr = fettlr_with(runner, &tree);

        fettlr.refresh().expect("refresh succeeds");
        let snapshot = fettlr.check_updates().expect("check_updates succeeds");

        let report = snapshot.update_report.expect("report");
        assert_eq!(
            report.plugins["accesslint@accesslint#user"],
            UpdateState::UpdateAvailable {
                installed: "0.10.2".to_string(),
                latest: "0.10.3".to_string()
            }
        );
        assert!(
            report
                .stale_ids()
                .plugins
                .contains(&"accesslint@accesslint#user".to_string())
        );
    }
}

mod mutations {
    use super::*;

    fn plugin_skill_snapshot(
        tree: &TempTree,
        folder: &str,
        frontmatter_name: &str,
    ) -> (Fettlr, fettlr_core::Skill) {
        tree.make_skill(
            folder,
            &tree.plugin_skills_dir("impeccable"),
            Some(&TempTree::skill_markdown(frontmatter_name, "d")),
            false,
        );
        let install_path = tree.plugin_install_dir("impeccable").display().to_string();
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_plugin_list(&format!(
            r#"[{{"id": "impeccable@impeccable", "version": "4.7.1", "scope": "user", "enabled": true, "installPath": "{install_path}"}}]"#
        ));
        let fettlr = fettlr_with(runner, tree);
        let snapshot = fettlr.refresh().expect("refresh");
        let skill = snapshot
            .skills
            .into_iter()
            .find(|skill| skill.name == folder)
            .expect("skill listed");
        (fettlr, skill)
    }

    fn zip_entries(path: &Path) -> Vec<String> {
        let archive = zip::ZipArchive::new(std::fs::File::open(path).expect("open")).expect("zip");
        archive
            .file_names()
            .map(|name| name.expect("name").into_owned())
            .collect()
    }

    #[test]
    fn package_skill_writes_into_the_packages_folder_under_the_skills_name() {
        let tree = TempTree::new();
        let (fettlr, skill) = plugin_skill_snapshot(&tree, "impeccable", "impeccable");

        let path = fettlr.package_skill(&skill).expect("package");

        assert_eq!(path, tree.paths.skill_packages_dir().join("impeccable.zip"));
        assert!(
            zip_entries(&path)
                .iter()
                .all(|name| name.starts_with("impeccable/"))
        );
    }

    #[test]
    fn the_zip_root_is_the_frontmatter_name() {
        let tree = TempTree::new();
        let (fettlr, skill) = plugin_skill_snapshot(&tree, "my-skill", "house-style");

        let path = fettlr.package_skill(&skill).expect("package");

        assert!(path.ends_with("house-style.zip"));
        assert!(
            zip_entries(&path)
                .iter()
                .all(|name| name.starts_with("house-style/"))
        );
    }

    #[test]
    fn an_account_skill_cannot_be_packaged() {
        let tree = TempTree::new();
        let (_, runner) = account_and_plugin_copy(&tree);
        let fettlr = fettlr_with(runner, &tree);
        let snapshot = fettlr.refresh().expect("refresh");
        let account = snapshot
            .skills
            .into_iter()
            .find(|skill| skill.id == "account:impeccable")
            .expect("account skill");

        let result = fettlr.package_skill(&account);

        assert!(matches!(
            result,
            Err(CoreError::SkillManagedByAccount { .. })
        ));
    }

    #[test]
    fn update_all_never_touches_an_account_skill() {
        let tree = TempTree::new();
        let (account, runner) = account_and_plugin_copy(&tree);
        let before = std::fs::read(account.join("SKILL.md")).expect("read");
        let fettlr = fettlr_with(runner, &tree);
        fettlr.refresh().expect("refresh");
        fettlr.check_updates().expect("check");

        let progress = Arc::new(Mutex::new(Vec::new()));
        let recorder = progress.clone();
        let snapshot = fettlr
            .update_all(&move |update: UpdateProgress| {
                recorder.lock().expect("lock poisoned").push(update.id);
            })
            .expect("update_all");

        assert!(
            !progress
                .lock()
                .expect("lock poisoned")
                .contains(&"account:impeccable".to_string())
        );
        assert!(snapshot.update_failures.is_empty());
        assert_eq!(
            std::fs::read(account.join("SKILL.md")).expect("read"),
            before
        );
    }

    #[test]
    fn a_mutation_re_reads_and_reflects_the_new_state() {
        let tree = TempTree::new();
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_plugin_list(PLUGIN_LIST);
        let fettlr = fettlr_with(runner.clone(), &tree);

        let baseline = fettlr.refresh().expect("baseline refresh");
        assert!(baseline.plugins[0].enabled);

        runner.set_plugin_list(
            r#"[
  {
    "id": "widgets@acme",
    "version": "1.0.0",
    "scope": "user",
    "enabled": false,
    "installPath": "/fake/home/.claude/plugins/cache/acme/widgets/1.0.0"
  }
]"#,
        );

        let after = fettlr
            .set_plugin_enabled("widgets@acme", false, &Scope::User)
            .expect("mutation succeeds");

        assert!(!after.plugins[0].enabled);
        assert!(
            runner
                .calls()
                .iter()
                .any(|call| call.starts_with(&["plugin".to_string(), "disable".to_string()]))
        );
    }

    #[test]
    fn pull_skill_pulls_a_git_clone_and_re_reads() {
        let tree = TempTree::new();
        tree.make_skill(
            "cloned-skill",
            &tree.user_skills_dir(),
            Some(&TempTree::skill_markdown("cloned-skill", "A cloned skill")),
            true,
        );
        let runner = Arc::new(ScriptedCliRunner::new());
        let fettlr = fettlr_with(runner, &tree);
        let skill = fettlr.refresh().expect("refresh").skills[0].clone();

        let snapshot = fettlr.pull_skill(&skill).expect("pull succeeds");

        assert_eq!(snapshot.skills.len(), 1);
    }

    #[test]
    fn pull_skill_refuses_a_skill_that_is_not_a_git_clone() {
        let tree = TempTree::new();
        tree.make_skill(
            "local-skill",
            &tree.user_skills_dir(),
            Some(&TempTree::skill_markdown("local-skill", "Not a clone")),
            false,
        );
        let runner = Arc::new(ScriptedCliRunner::new());
        let fettlr = fettlr_with(runner, &tree);
        let skill = fettlr.refresh().expect("refresh").skills[0].clone();

        let error = fettlr
            .pull_skill(&skill)
            .expect_err("a non-git-clone skill has nothing to pull");

        assert!(matches!(error, CoreError::NotAGitClone { .. }));
    }

    #[test]
    fn update_all_goes_marketplaces_then_plugins_then_git_skills_and_records_one_failure() {
        let tree = TempTree::new();
        let skill_path = tree.make_skill(
            "my-skill",
            &tree.user_skills_dir(),
            Some(&TempTree::skill_markdown("my-skill", "A cloned skill")),
            true,
        );

        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_marketplace_list(MARKETPLACE_LIST);
        runner.set_plugin_list(
            r#"{
  "installed": [
    {
      "id": "widgets@acme",
      "version": "1.0.0",
      "scope": "user",
      "enabled": true,
      "installPath": "/fake/home/.claude/plugins/cache/acme/widgets/1.0.0"
    }
  ],
  "available": []
}"#,
        );
        runner.fail_when_args_start_with(&["plugin", "update", "widgets@acme"]);

        let git = Arc::new(FakeGitProbe::with_counts([
            ("/fake/marketplaces/acme".to_string(), 3),
            (skill_path.display().to_string(), 2),
        ]));

        tree.write_marketplace(
            "acme",
            json!({"source": "github", "repo": "acme/marketplace"}),
            json!([{"name": "widgets", "version": "2.0.0", "source": "./plugins/widgets"}]),
        );

        let mut paths = tree.paths.clone();
        paths.binary_override = Some(fake_binary_path());
        let fettlr = Fettlr::new(
            paths,
            runner.clone(),
            git,
            Arc::new(FakeRegistryClient::new()),
            Arc::new(FakeTrasher::new()),
        );

        fettlr.refresh().expect("refresh succeeds");
        fettlr.check_updates().expect("check_updates succeeds");

        let current = fettlr.snapshot();
        let plugin_id = current
            .plugins
            .iter()
            .find(|plugin| plugin.name == "widgets")
            .expect("widgets plugin")
            .id();
        let skill_id = current
            .skills
            .iter()
            .find(|skill| skill.name == "my-skill")
            .expect("my-skill")
            .id
            .clone();

        let progress = Arc::new(Mutex::new(Vec::new()));
        let recorder = progress.clone();
        let snapshot = fettlr
            .update_all(&move |update: UpdateProgress| {
                recorder.lock().expect("lock poisoned").push((
                    update.kind,
                    update.id,
                    update.stage,
                ));
            })
            .expect("update_all succeeds even with one failed item");

        let recorded = progress.lock().expect("lock poisoned").clone();
        assert_eq!(
            recorded,
            vec![
                (
                    UpdateItemKind::Marketplace,
                    "acme".to_string(),
                    UpdateStage::Running
                ),
                (
                    UpdateItemKind::Marketplace,
                    "acme".to_string(),
                    UpdateStage::Updated
                ),
                (
                    UpdateItemKind::Plugin,
                    plugin_id.clone(),
                    UpdateStage::Running
                ),
                (UpdateItemKind::Plugin, plugin_id, UpdateStage::Failed),
                (
                    UpdateItemKind::Skill,
                    skill_id.clone(),
                    UpdateStage::Running
                ),
                (UpdateItemKind::Skill, skill_id, UpdateStage::Updated),
            ]
        );
        assert_eq!(snapshot.update_failures.len(), 1);
        let failure = &snapshot.update_failures[0];
        assert_eq!(failure.kind, UpdateItemKind::Plugin);
        assert_eq!(failure.name, "widgets");
        assert!(!failure.error.is_empty());
        assert!(!snapshot.warnings.iter().any(|w| w.contains("widgets")));
    }

    /// A run that clears its whole list leaves nothing behind for the window to read as a failure.
    #[test]
    fn update_all_reports_no_failures_when_every_item_succeeds() {
        let tree = TempTree::new();
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_marketplace_list(MARKETPLACE_LIST);
        runner.set_plugin_list("{}");

        let git = Arc::new(FakeGitProbe::with_counts([(
            "/fake/marketplaces/acme".to_string(),
            3,
        )]));

        let mut paths = tree.paths.clone();
        paths.binary_override = Some(fake_binary_path());
        let fettlr = Fettlr::new(
            paths,
            runner.clone(),
            git,
            Arc::new(FakeRegistryClient::new()),
            Arc::new(FakeTrasher::new()),
        );

        fettlr.refresh().expect("refresh succeeds");
        fettlr.check_updates().expect("check_updates succeeds");
        let snapshot = fettlr
            .update_all(&|_: UpdateProgress| {})
            .expect("update_all succeeds");

        assert!(snapshot.update_failures.is_empty());
    }

    /// The failures belong to the run that produced them, not to every snapshot after it.
    #[test]
    fn a_later_refresh_clears_the_failures_of_the_last_run() {
        let tree = TempTree::new();
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_marketplace_list(MARKETPLACE_LIST);
        runner.set_plugin_list("{}");
        runner.fail_when_args_start_with(&["plugin", "marketplace", "update", "acme"]);

        let git = Arc::new(FakeGitProbe::with_counts([(
            "/fake/marketplaces/acme".to_string(),
            3,
        )]));

        let mut paths = tree.paths.clone();
        paths.binary_override = Some(fake_binary_path());
        let fettlr = Fettlr::new(
            paths,
            runner.clone(),
            git,
            Arc::new(FakeRegistryClient::new()),
            Arc::new(FakeTrasher::new()),
        );

        fettlr.refresh().expect("refresh succeeds");
        fettlr.check_updates().expect("check_updates succeeds");
        let run = fettlr
            .update_all(&|_: UpdateProgress| {})
            .expect("update_all succeeds");
        assert_eq!(run.update_failures.len(), 1);
        assert_eq!(run.update_failures[0].kind, UpdateItemKind::Marketplace);

        let refreshed = fettlr.refresh().expect("refresh succeeds");
        assert!(refreshed.update_failures.is_empty());
    }

    #[test]
    fn update_all_leaves_alone_a_plugin_whose_catalogue_names_no_version() {
        let tree = TempTree::new();
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_marketplace_list("[]");
        runner.set_plugin_list(
            r#"{
  "installed": [
    {
      "id": "widgets@acme",
      "version": "1.0.0",
      "scope": "user",
      "enabled": true,
      "installPath": "/fake/home/.claude/plugins/cache/acme/widgets/1.0.0"
    }
  ],
  "available": []
}"#,
        );

        tree.write_marketplace(
            "acme",
            json!({"source": "github", "repo": "acme/marketplace"}),
            json!([{"name": "widgets", "source": "./plugins/widgets"}]),
        );

        let mut paths = tree.paths.clone();
        paths.binary_override = Some(fake_binary_path());
        let fettlr = Fettlr::new(
            paths,
            runner.clone(),
            Arc::new(FakeGitProbe::new()),
            Arc::new(FakeRegistryClient::new()),
            Arc::new(FakeTrasher::new()),
        );

        fettlr.refresh().expect("refresh succeeds");
        fettlr.check_updates().expect("check_updates succeeds");
        fettlr
            .update_all(&|_: UpdateProgress| {})
            .expect("update_all succeeds");

        assert!(
            !runner
                .calls()
                .iter()
                .any(|call| call.starts_with(&["plugin".to_string(), "update".to_string()]))
        );
    }

    #[test]
    fn update_all_skips_a_stale_plugin_in_a_scope_it_cannot_update() {
        let tree = TempTree::new();
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_marketplace_list("[]");
        runner.set_plugin_list(
            r#"{
  "installed": [
    {
      "id": "widgets@acme",
      "version": "1.0.0",
      "scope": "synced",
      "enabled": true,
      "installPath": "/fake/home/.claude/plugins/cache/acme/widgets/1.0.0"
    }
  ],
  "available": []
}"#,
        );

        tree.write_marketplace(
            "acme",
            json!({"source": "github", "repo": "acme/marketplace"}),
            json!([{"name": "widgets", "version": "2.0.0", "source": "./plugins/widgets"}]),
        );

        let mut paths = tree.paths.clone();
        paths.binary_override = Some(fake_binary_path());
        let fettlr = Fettlr::new(
            paths,
            runner.clone(),
            Arc::new(FakeGitProbe::new()),
            Arc::new(FakeRegistryClient::new()),
            Arc::new(FakeTrasher::new()),
        );

        fettlr.refresh().expect("refresh succeeds");
        fettlr.check_updates().expect("check_updates succeeds");
        let snapshot = fettlr
            .update_all(&|_: UpdateProgress| {})
            .expect("update_all succeeds");

        assert!(snapshot.update_failures.is_empty());
        assert!(
            !runner
                .calls()
                .iter()
                .any(|call| call.starts_with(&["plugin".to_string(), "update".to_string()]))
        );
    }
}

mod watching {
    use super::*;

    fn watch_ready_paths() -> TempTree {
        let tree = TempTree::new();
        std::fs::create_dir_all(tree.paths.skills_dir()).expect("create skills dir");
        std::fs::create_dir_all(tree.paths.plugins_dir()).expect("create plugins dir");
        std::fs::create_dir_all(tree.paths.agents_dir()).expect("create agents dir");
        std::fs::write(&tree.paths.claude_json, "{}").expect("touch claude.json");
        tree
    }

    #[test]
    fn a_watched_change_fires_the_callback_within_a_bounded_wait() {
        let tree = watch_ready_paths();
        let count = Arc::new(AtomicUsize::new(0));
        let handler_count = count.clone();
        let _watcher = Watcher::with_throttle(
            &tree.paths,
            Box::new(move || {
                handler_count.fetch_add(1, Ordering::SeqCst);
            }),
            Duration::from_millis(300),
        )
        .expect("watcher starts");

        std::fs::write(tree.paths.skills_dir().join("marker.txt"), "hi").expect("write marker");

        assert!(
            wait_until(Duration::from_secs(5), || count.load(Ordering::SeqCst) >= 1),
            "expected a callback for a change under the watched skills directory"
        );
    }

    #[test]
    fn a_change_under_the_agents_directory_fires_the_callback() {
        let tree = watch_ready_paths();
        let count = Arc::new(AtomicUsize::new(0));
        let handler_count = count.clone();
        let _watcher = Watcher::with_throttle(
            &tree.paths,
            Box::new(move || {
                handler_count.fetch_add(1, Ordering::SeqCst);
            }),
            Duration::from_millis(300),
        )
        .expect("watcher starts");

        std::fs::write(tree.paths.agents_dir().join("marker.txt"), "hi").expect("write marker");

        assert!(
            wait_until(Duration::from_secs(5), || count.load(Ordering::SeqCst) >= 1),
            "expected a callback for a change under the watched agents directory"
        );
    }

    #[test]
    fn two_touches_inside_the_throttle_window_produce_one_callback() {
        let tree = watch_ready_paths();
        let count = Arc::new(AtomicUsize::new(0));
        let handler_count = count.clone();
        let _watcher = Watcher::with_throttle(
            &tree.paths,
            Box::new(move || {
                handler_count.fetch_add(1, Ordering::SeqCst);
            }),
            Duration::from_millis(400),
        )
        .expect("watcher starts");

        std::fs::write(tree.paths.skills_dir().join("first.txt"), "1").expect("write first");
        assert!(
            wait_until(Duration::from_secs(5), || count.load(Ordering::SeqCst) >= 1),
            "expected the first, immediate callback"
        );

        std::fs::write(tree.paths.skills_dir().join("second.txt"), "2").expect("write second");
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(tree.paths.skills_dir().join("third.txt"), "3").expect("write third");

        assert!(
            wait_until(Duration::from_secs(5), || count.load(Ordering::SeqCst) >= 2),
            "expected one trailing callback for the two coalesced touches"
        );
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(
            count.load(Ordering::SeqCst),
            2,
            "two touches inside the same window must coalesce into a single trailing callback"
        );
    }

    #[test]
    fn a_change_during_a_slow_callback_is_not_lost() {
        let tree = watch_ready_paths();
        let count = Arc::new(AtomicUsize::new(0));
        let handler_count = count.clone();
        let _watcher = Watcher::with_throttle(
            &tree.paths,
            Box::new(move || {
                handler_count.fetch_add(1, Ordering::SeqCst);
                // Stands in for a refresh that outlasts its own throttle window.
                std::thread::sleep(Duration::from_millis(400));
            }),
            Duration::from_millis(100),
        )
        .expect("watcher starts");

        std::fs::write(tree.paths.skills_dir().join("first.txt"), "1").expect("write first");
        assert!(
            wait_until(Duration::from_secs(5), || count.load(Ordering::SeqCst) >= 1),
            "expected the first, immediate callback"
        );

        // Lands while the first callback is still inside its sleep.
        std::thread::sleep(Duration::from_millis(100));
        std::fs::write(tree.paths.skills_dir().join("second.txt"), "2").expect("write second");

        assert!(
            wait_until(Duration::from_secs(5), || count.load(Ordering::SeqCst) >= 2),
            "a change arriving while the callback ran was dropped instead of coalesced"
        );
        std::thread::sleep(Duration::from_millis(800));
        assert_eq!(
            count.load(Ordering::SeqCst),
            2,
            "the change should have produced one follow-up callback, not a queue of them"
        );
    }

    #[test]
    fn dropping_the_watcher_cancels_a_still_pending_throttled_callback() {
        let tree = watch_ready_paths();
        let (sender, receiver) = std::sync::mpsc::channel::<()>();
        let watcher = Watcher::with_throttle(
            &tree.paths,
            Box::new(move || {
                let _ = sender.send(());
            }),
            Duration::from_millis(500),
        )
        .expect("watcher starts");

        // The first touch fires immediately (nothing to throttle against yet); draining it first
        // is what puts the second touch into an actually-pending, still-sleeping throttle window
        // rather than racing FSEvents' own delivery latency against the drop below.
        std::fs::write(tree.paths.skills_dir().join("first.txt"), "1").expect("write first");
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("expected the first, immediate callback");

        std::fs::write(tree.paths.skills_dir().join("second.txt"), "2").expect("write second");
        std::thread::sleep(Duration::from_millis(50));
        drop(watcher);

        let result = receiver.recv_timeout(Duration::from_millis(700));
        assert!(
            result.is_err(),
            "a throttled callback still pending when the watcher is dropped must never fire"
        );
    }
}

#[test]
fn check_updates_does_not_undo_a_refresh_that_lands_while_it_runs() {
    let tree = TempTree::new();
    let runner = Arc::new(ScriptedCliRunner::new());
    runner.set_plugin_list(PLUGIN_LIST);
    let fettlr = Arc::new(fettlr_with(runner.clone(), &tree));

    fettlr.refresh().expect("first refresh");
    assert_eq!(fettlr.snapshot().plugins.len(), 1);

    // Pause inside the catalogue call, which check_updates makes after it has read the lists it
    // is about to check and before it has anything to write back.
    let (entered, release) = runner.pause_when_args_start_with(&["plugin", "list", "--json"]);

    let checking = {
        let fettlr = fettlr.clone();
        std::thread::spawn(move || fettlr.check_updates().expect("check updates"))
    };

    entered.recv().expect("the catalogue call starts");
    runner.set_plugin_list("[]");
    fettlr
        .refresh()
        .expect("the plugin is uninstalled mid-check");
    assert!(fettlr.snapshot().plugins.is_empty());
    release.send(()).expect("let the catalogue call finish");
    let returned = checking.join().expect("the check finishes");

    assert!(
        fettlr.snapshot().plugins.is_empty(),
        "the cache kept a plugin the refresh had already removed"
    );
    assert!(
        returned.plugins.is_empty(),
        "the window was handed a plugin the refresh had already removed"
    );
    assert!(returned.update_report.is_some());
}

/// Runs the real `claude mcp list` on this machine. Ignored by default: it health checks every MCP
/// server, which means network calls and spawning every stdio server, and it needs a real `claude`.
#[test]
#[ignore]
fn real_claude_mcp_list_parses() {
    let tree = TempTree::new();
    let mut paths = tree.paths.clone();
    paths.binary_override = None;
    let fettlr = Fettlr::new(
        paths,
        Arc::new(fettlr_core::ProcessCliRunner),
        Arc::new(FakeGitProbe::new()),
        Arc::new(FakeRegistryClient::new()),
        Arc::new(FakeTrasher::new()),
    );

    let snapshot = fettlr.refresh_connectors().expect("reads the connectors");
    println!("checked_at: {:?}", snapshot.connectors_checked_at);
    for connector in &snapshot.connectors {
        println!(
            "  {} | {} | {}",
            connector.label,
            connector.target.as_deref().unwrap_or("-"),
            connector.status.as_deref().unwrap_or("-")
        );
    }
    assert!(snapshot.connectors_checked_at.is_some());
}

mod subagents {
    use super::*;
    use fettlr_core::testing::{FakeGitProbe, FakeRegistryClient, FakeTrasher};
    use fettlr_core::{CoreError, Fettlr, Provenance, Subagent};

    fn scripted_runner() -> Arc<ScriptedCliRunner> {
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_plugin_list(PLUGIN_LIST);
        runner.set_marketplace_list(MARKETPLACE_LIST);
        runner
    }

    fn fettlr_with_trasher(tree: &TempTree, trasher: Arc<FakeTrasher>) -> Fettlr {
        let mut paths = tree.paths.clone();
        paths.binary_override = Some(fake_binary_path());
        Fettlr::new(
            paths,
            scripted_runner(),
            Arc::new(FakeGitProbe::new()),
            Arc::new(FakeRegistryClient::new()),
            trasher,
        )
    }

    #[test]
    fn refresh_lists_active_and_parked_subagents() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        tree.make_subagent(
            "reviewer",
            &tree.user_agents_dir(),
            Some(&TempTree::subagent_markdown("reviewer", "Reviews.")),
        );
        let fettlr = fettlr_with(scripted_runner(), &tree);

        let snapshot = fettlr.refresh().expect("refresh");
        assert_eq!(snapshot.subagents.len(), 1);
        assert_eq!(snapshot.subagents[0].name, "reviewer");
        assert!(snapshot.parked_subagents.is_empty());

        let subagent = snapshot.subagents[0].clone();
        let after_park = fettlr.park_subagent(&subagent).expect("park");
        assert!(after_park.subagents.is_empty());
        assert_eq!(after_park.parked_subagents.len(), 1);
        assert_eq!(after_park.parked_subagents[0].name, "reviewer");

        let parked = after_park.parked_subagents[0].clone();
        let after_restore = fettlr.restore_subagent(&parked).expect("restore");
        assert_eq!(after_restore.subagents.len(), 1);
        assert!(after_restore.parked_subagents.is_empty());
    }

    #[test]
    fn an_account_skill_cannot_be_parked_or_deleted() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        tree.write_account_manifest(
            "b1",
            1,
            json!([{"name": "impeccable", "source": "plugin", "updatedAt": "2026-10-24T12:57:30Z"}]),
        );
        let folder = tree.make_account_skill(
            "b1",
            "impeccable",
            &TempTree::skill_markdown("impeccable", "Design skill"),
        );
        let trasher = Arc::new(FakeTrasher::new());
        let fettlr = fettlr_with_trasher(&tree, trasher.clone());
        let snapshot = fettlr.refresh().expect("refresh");
        let skill = snapshot
            .skills
            .iter()
            .find(|skill| skill.id == "account:impeccable")
            .expect("listed")
            .clone();

        assert!(matches!(
            fettlr.park_skill(&skill),
            Err(CoreError::SkillManagedByAccount { .. })
        ));
        assert!(matches!(
            fettlr.delete_skill(&skill),
            Err(CoreError::SkillManagedByAccount { .. })
        ));
        assert!(folder.exists());
        assert!(trasher.trashed().is_empty());
    }

    #[test]
    fn an_account_skill_cannot_be_pulled() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        tree.write_account_manifest(
            "b1",
            1,
            json!([{"name": "impeccable", "source": "plugin", "updatedAt": "2026-10-24T12:57:30Z"}]),
        );
        tree.make_account_skill(
            "b1",
            "impeccable",
            &TempTree::skill_markdown("impeccable", "Design skill"),
        );
        let fettlr = fettlr_with(scripted_runner(), &tree);
        let snapshot = fettlr.refresh().expect("refresh");
        let skill = snapshot
            .skills
            .iter()
            .find(|skill| skill.id == "account:impeccable")
            .expect("listed")
            .clone();

        assert!(matches!(
            fettlr.pull_skill(&skill),
            Err(CoreError::SkillManagedByAccount { .. })
        ));
    }

    #[test]
    fn a_plugin_owned_subagent_cannot_be_parked_or_deleted() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        let fettlr = fettlr_with(scripted_runner(), &tree);
        fettlr.refresh().expect("refresh");
        let subagent = Subagent {
            id: "plugin:acme@shop::auditor".to_string(),
            relative_path: "auditor.md".to_string(),
            name: "auditor".to_string(),
            summary: None,
            model: None,
            tools: Vec::new(),
            effort: None,
            permission_mode: None,
            max_turns: None,
            memory: None,
            scope: Scope::User,
            provenance: Provenance::Plugin {
                plugin_id: "acme@shop".to_string(),
            },
            path: tree.root().join("never-touched.md"),
        };

        assert!(matches!(
            fettlr.park_subagent(&subagent),
            Err(CoreError::SubagentManagedByPlugin { .. })
        ));
        assert!(matches!(
            fettlr.delete_subagent(&subagent),
            Err(CoreError::SubagentManagedByPlugin { .. })
        ));
    }

    #[test]
    fn a_nested_subagent_parks_under_its_relative_path() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        let file = tree.make_subagent(
            "security",
            &tree.user_agents_dir().join("review"),
            Some(&TempTree::subagent_markdown("security", "Reviews.")),
        );
        let fettlr = fettlr_with(scripted_runner(), &tree);
        let subagent = fettlr.refresh().expect("refresh").subagents[0].clone();

        let after_park = fettlr.park_subagent(&subagent).expect("park");

        let parked = after_park.parked_subagents[0].clone();
        assert!(
            parked
                .parked_path
                .ends_with("agents-disabled/user/review/security.md")
        );
        assert!(!file.exists());

        let after_restore = fettlr.restore_subagent(&parked).expect("restore");
        assert!(file.is_file());
        assert!(after_restore.parked_subagents.is_empty());
    }

    #[test]
    fn delete_subagent_sends_the_file_to_the_trash() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        let file = tree.make_subagent("reviewer", &tree.user_agents_dir(), None);
        let trasher = Arc::new(FakeTrasher::new());
        let fettlr = fettlr_with_trasher(&tree, trasher.clone());
        let subagent = fettlr.refresh().expect("refresh").subagents[0].clone();

        fettlr.delete_subagent(&subagent).expect("delete");

        assert_eq!(trasher.trashed(), vec![file]);
    }
}

mod skills_cli_updates {
    use std::collections::BTreeMap;

    use super::*;
    use fettlr_core::{Provenance, Skill, SkillsCliOrigin, Trashing};

    const URL: &str = "https://github.com/emilkowalski/skills.git";
    const OLD_HASH: &str = "1111111111111111111111111111111111111111";
    const NEW_HASH: &str = "2222222222222222222222222222222222222222";
    const COMMIT: &str = "cccccccccccccccccccccccccccccccccccccccc";
    const OLD_UPDATED_AT: &str = "2026-01-02T00:00:00.000Z";

    fn lock_entry() -> serde_json::Value {
        json!({
            "source": "emilkowalski/skills",
            "sourceType": "github",
            "sourceUrl": URL,
            "skillPath": "skills/animate/SKILL.md",
            "skillFolderHash": OLD_HASH,
            "installedAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": OLD_UPDATED_AT
        })
    }

    fn source_probe() -> FakeGitProbe {
        FakeGitProbe::with_sources([(URL, COMMIT, BTreeMap::from([("skills/animate", NEW_HASH)]))])
    }

    fn fettlr_for(tree: &TempTree, git: Arc<FakeGitProbe>, trasher: Arc<FakeTrasher>) -> Fettlr {
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_plugin_list("{}");
        let mut paths = tree.paths.clone();
        paths.binary_override = Some(fake_binary_path());
        Fettlr::new(
            paths,
            runner,
            git,
            Arc::new(FakeRegistryClient::new()),
            trasher,
        )
    }

    fn installed_tree() -> TempTree {
        let tree = TempTree::new();
        tree.make_skills_cli_skill("animate");
        tree.write_skills_lock(json!({"animate": lock_entry()}));
        tree
    }

    fn animate(fettlr: &Fettlr) -> Skill {
        fettlr
            .snapshot()
            .skills
            .into_iter()
            .find(|skill| skill.name == "animate")
            .expect("animate is listed")
    }

    fn lock_entry_on_disk(tree: &TempTree) -> serde_json::Value {
        let raw = std::fs::read_to_string(tree.paths.skills_cli_lock()).expect("read lock");
        let root: serde_json::Value = serde_json::from_str(&raw).expect("parse lock");
        root["skills"]["animate"].clone()
    }

    fn set_aside_folder(tree: &TempTree) -> PathBuf {
        tree.paths
            .skills_cli_dir()
            .join(".fettlr-replaced")
            .join("animate")
    }

    fn canonical_skill_md(tree: &TempTree) -> PathBuf {
        tree.paths
            .skills_cli_skills_dir()
            .join("animate")
            .join("SKILL.md")
    }

    #[test]
    fn updating_a_skills_cli_skill_replaces_its_folder_and_records_the_hash() {
        let tree = installed_tree();
        let link = tree.paths.skills_dir().join("animate");
        let link_target = std::fs::read_link(&link).expect("read link");
        let trasher = Arc::new(FakeTrasher::new());
        let fettlr = fettlr_for(&tree, Arc::new(source_probe()), trasher.clone());
        fettlr.refresh().expect("refresh");
        fettlr.check_updates().expect("check");
        let skill = animate(&fettlr);
        assert!(matches!(skill.provenance, Provenance::SkillsCli(_)));

        fettlr.pull_skill(&skill).expect("update");

        assert_eq!(
            std::fs::read_to_string(canonical_skill_md(&tree)).expect("read skill"),
            format!("exported {COMMIT}")
        );
        let entry = lock_entry_on_disk(&tree);
        assert_eq!(entry["skillFolderHash"], NEW_HASH);
        assert_ne!(entry["updatedAt"], OLD_UPDATED_AT);
        assert_eq!(std::fs::read_link(&link).expect("read link"), link_target);
        assert_eq!(trasher.trashed(), vec![set_aside_folder(&tree)]);
    }

    #[test]
    fn update_all_updates_a_stale_skills_cli_skill() {
        let tree = installed_tree();
        let fettlr = fettlr_for(
            &tree,
            Arc::new(source_probe()),
            Arc::new(FakeTrasher::new()),
        );
        fettlr.refresh().expect("refresh");
        fettlr.check_updates().expect("check");

        let progress = Arc::new(Mutex::new(Vec::new()));
        let recorder = progress.clone();
        let snapshot = fettlr
            .update_all(&move |update: UpdateProgress| {
                recorder.lock().expect("lock poisoned").push((
                    update.kind,
                    update.id,
                    update.stage,
                ));
            })
            .expect("update_all");

        assert_eq!(
            progress.lock().expect("lock poisoned").clone(),
            vec![
                (
                    UpdateItemKind::Skill,
                    "user/animate".to_string(),
                    UpdateStage::Running
                ),
                (
                    UpdateItemKind::Skill,
                    "user/animate".to_string(),
                    UpdateStage::Updated
                ),
            ]
        );
        assert!(snapshot.update_failures.is_empty());
        assert_eq!(lock_entry_on_disk(&tree)["skillFolderHash"], NEW_HASH);
    }

    #[test]
    fn updating_a_skill_whose_source_cannot_be_fetched_reports_it() {
        let tree = installed_tree();
        let lock_before = std::fs::read(tree.paths.skills_cli_lock()).expect("read lock");
        let skill_before = std::fs::read(canonical_skill_md(&tree)).expect("read skill");
        let git = Arc::new(FakeGitProbe::new().with_unreachable_source(URL));
        let fettlr = fettlr_for(&tree, git.clone(), Arc::new(FakeTrasher::new()));
        fettlr.refresh().expect("refresh");
        let skill = animate(&fettlr);

        assert!(fettlr.pull_skill(&skill).is_err());
        assert_eq!(git.fetch_count(URL), 1);

        assert_eq!(
            std::fs::read(tree.paths.skills_cli_lock()).expect("read lock"),
            lock_before
        );
        assert_eq!(
            std::fs::read(canonical_skill_md(&tree)).expect("read skill"),
            skill_before
        );
    }

    #[derive(Default)]
    struct RemovingTrasher {
        trashed: Mutex<Vec<PathBuf>>,
    }

    impl Trashing for RemovingTrasher {
        fn trash(&self, path: &Path) -> Result<(), CoreError> {
            std::fs::remove_dir_all(path)?;
            self.trashed
                .lock()
                .expect("lock poisoned")
                .push(path.to_path_buf());
            Ok(())
        }
    }

    #[test]
    fn an_update_after_a_crash_past_the_swap_converges() {
        let tree = installed_tree();
        tree.write(&format!("exported {COMMIT}"), &canonical_skill_md(&tree));
        let set_aside = set_aside_folder(&tree);
        tree.write("# animate\n", &set_aside.join("SKILL.md"));
        let trasher = Arc::new(RemovingTrasher::default());
        let runner = Arc::new(ScriptedCliRunner::new());
        runner.set_plugin_list("{}");
        let mut paths = tree.paths.clone();
        paths.binary_override = Some(fake_binary_path());
        let fettlr = Fettlr::new(
            paths,
            runner,
            Arc::new(source_probe()),
            Arc::new(FakeRegistryClient::new()),
            trasher.clone(),
        );
        fettlr.refresh().expect("refresh");

        fettlr.pull_skill(&animate(&fettlr)).expect("update");

        assert_eq!(
            std::fs::read_to_string(canonical_skill_md(&tree)).expect("read skill"),
            format!("exported {COMMIT}")
        );
        let entry = lock_entry_on_disk(&tree);
        assert_eq!(entry["skillFolderHash"], NEW_HASH);
        assert_ne!(entry["updatedAt"], OLD_UPDATED_AT);
        assert!(!set_aside.exists());
        assert!(!tree.paths.skills_cli_dir().join(".fettlr-staging").exists());
        assert_eq!(
            trasher.trashed.lock().expect("lock poisoned").clone(),
            vec![set_aside.clone(), set_aside]
        );
    }

    #[test]
    fn an_update_whose_lock_hash_already_matches_writes_nothing() {
        let tree = TempTree::new();
        tree.make_skills_cli_skill("animate");
        let mut current = lock_entry();
        current["skillFolderHash"] = json!(NEW_HASH);
        tree.write_skills_lock(json!({"animate": current}));
        let skill_before = std::fs::read(canonical_skill_md(&tree)).expect("read skill");
        let lock_before = std::fs::read(tree.paths.skills_cli_lock()).expect("read lock");
        let git = Arc::new(source_probe());
        let trasher = Arc::new(FakeTrasher::new());
        let fettlr = fettlr_for(&tree, git.clone(), trasher.clone());
        fettlr.refresh().expect("refresh");

        fettlr.pull_skill(&animate(&fettlr)).expect("update");

        assert!(git.exports().is_empty());
        assert!(trasher.trashed().is_empty());
        assert_eq!(
            std::fs::read(canonical_skill_md(&tree)).expect("read skill"),
            skill_before
        );
        assert_eq!(
            std::fs::read(tree.paths.skills_cli_lock()).expect("read lock"),
            lock_before
        );
    }

    #[test]
    fn leftover_staging_is_cleared_before_an_update() {
        let tree = installed_tree();
        let junk = tree.write(
            "junk",
            &tree
                .paths
                .skills_cli_dir()
                .join(".fettlr-staging")
                .join("junk"),
        );
        let fettlr = fettlr_for(
            &tree,
            Arc::new(source_probe()),
            Arc::new(FakeTrasher::new()),
        );
        fettlr.refresh().expect("refresh");

        fettlr.pull_skill(&animate(&fettlr)).expect("update");

        assert!(!junk.exists());
        assert!(!tree.paths.skills_cli_dir().join(".fettlr-staging").exists());
        assert_eq!(
            std::fs::read_to_string(canonical_skill_md(&tree)).expect("read skill"),
            format!("exported {COMMIT}")
        );
    }

    #[test]
    fn a_skills_cli_lock_of_another_version_is_never_written() {
        let tree = TempTree::new();
        let link = tree.make_skills_cli_skill("animate");
        tree.write(
            &serde_json::to_string_pretty(&json!({
                "version": 4,
                "skills": {"animate": lock_entry()}
            }))
            .expect("serialize lock"),
            &tree.paths.skills_cli_lock(),
        );
        let lock_before = std::fs::read(tree.paths.skills_cli_lock()).expect("read lock");
        let git = Arc::new(source_probe());
        let fettlr = fettlr_for(&tree, git.clone(), Arc::new(FakeTrasher::new()));
        fettlr.refresh().expect("refresh");
        let skill = Skill {
            id: "user/animate".to_string(),
            name: "animate".to_string(),
            title: None,
            summary: None,
            scope: Scope::User,
            provenance: Provenance::SkillsCli(SkillsCliOrigin {
                source: "emilkowalski/skills".to_string(),
                source_type: "github".to_string(),
                source_url: Some(URL.to_string()),
                reference: None,
                folder: "skills/animate".to_string(),
                folder_hash: OLD_HASH.to_string(),
            }),
            path: link,
        };

        assert!(matches!(
            fettlr.pull_skill(&skill),
            Err(CoreError::UnsupportedSkillSource { .. })
        ));
        assert_eq!(
            std::fs::read(tree.paths.skills_cli_lock()).expect("read lock"),
            lock_before
        );
        assert!(git.exports().is_empty());
        assert_eq!(git.fetch_count(URL), 0);
    }

    #[test]
    fn a_skills_cli_skill_whose_canonical_folder_is_a_link_is_refused() {
        let tree = TempTree::new();
        let elsewhere = tree.root().join("elsewhere").join("animate");
        let real_skill_md = tree.write("# animate\n", &elsewhere.join("SKILL.md"));
        tree.make_skill_symlink("animate", &tree.paths.skills_cli_skills_dir(), &elsewhere);
        let link = tree.make_skill_symlink(
            "animate",
            &tree.paths.skills_dir(),
            Path::new("../../.agents/skills/animate"),
        );
        tree.write_skills_lock(json!({"animate": lock_entry()}));
        let lock_before = std::fs::read(tree.paths.skills_cli_lock()).expect("read lock");
        let git = Arc::new(source_probe());
        let fettlr = fettlr_for(&tree, git.clone(), Arc::new(FakeTrasher::new()));
        fettlr.refresh().expect("refresh");
        assert_eq!(animate(&fettlr).provenance, Provenance::Local);

        assert!(matches!(
            fettlr.pull_skill(&hand_built("animate", link)),
            Err(CoreError::UnsupportedSkillSource { .. })
        ));
        assert_eq!(
            std::fs::read_to_string(&real_skill_md).expect("read skill"),
            "# animate\n"
        );
        assert!(
            std::fs::symlink_metadata(tree.paths.skills_cli_skills_dir().join("animate"))
                .expect("link metadata")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read(tree.paths.skills_cli_lock()).expect("read lock"),
            lock_before
        );
        assert!(git.exports().is_empty());
        assert_eq!(git.fetch_count(URL), 0);
    }

    #[test]
    fn a_folder_missing_upstream_is_reported_and_nothing_changes() {
        let tree = installed_tree();
        let lock_before = std::fs::read(tree.paths.skills_cli_lock()).expect("read lock");
        let git = Arc::new(FakeGitProbe::with_sources([(
            URL,
            COMMIT,
            BTreeMap::from([("skills/other", NEW_HASH)]),
        )]));
        let fettlr = fettlr_for(&tree, git, Arc::new(FakeTrasher::new()));
        fettlr.refresh().expect("refresh");

        assert!(matches!(
            fettlr.pull_skill(&animate(&fettlr)),
            Err(CoreError::SkillGoneUpstream { .. })
        ));
        assert_eq!(
            std::fs::read(tree.paths.skills_cli_lock()).expect("read lock"),
            lock_before
        );
    }

    fn hand_built(name: &str, path: PathBuf) -> Skill {
        Skill {
            id: format!("user/{name}"),
            name: name.to_string(),
            title: None,
            summary: None,
            scope: Scope::User,
            provenance: Provenance::SkillsCli(SkillsCliOrigin {
                source: "emilkowalski/skills".to_string(),
                source_type: "github".to_string(),
                source_url: Some(URL.to_string()),
                reference: None,
                folder: "skills/animate".to_string(),
                folder_hash: OLD_HASH.to_string(),
            }),
            path,
        }
    }

    #[test]
    fn a_lock_folder_that_climbs_out_of_the_source_is_refused() {
        let tree = TempTree::new();
        tree.make_skills_cli_skill("animate");
        let mut climbing = lock_entry();
        climbing["skillPath"] = json!("../../evil/SKILL.md");
        tree.write_skills_lock(json!({"animate": climbing}));
        let lock_before = std::fs::read(tree.paths.skills_cli_lock()).expect("read lock");
        let skill_before = std::fs::read(canonical_skill_md(&tree)).expect("read skill");
        let git = Arc::new(FakeGitProbe::with_sources([(
            URL,
            COMMIT,
            BTreeMap::from([("../../evil", NEW_HASH)]),
        )]));
        let fettlr = fettlr_for(&tree, git.clone(), Arc::new(FakeTrasher::new()));
        fettlr.refresh().expect("refresh");

        assert!(matches!(
            fettlr.pull_skill(&animate(&fettlr)),
            Err(CoreError::UnsupportedSkillSource { .. })
        ));
        assert_eq!(git.fetch_count(URL), 0);
        assert!(git.exports().is_empty());
        assert_eq!(
            std::fs::read(tree.paths.skills_cli_lock()).expect("read lock"),
            lock_before
        );
        assert_eq!(
            std::fs::read(canonical_skill_md(&tree)).expect("read skill"),
            skill_before
        );
    }

    #[test]
    fn a_skill_name_that_climbs_out_of_the_skills_folder_is_refused() {
        let tree = installed_tree();
        let mut entries = json!({"animate": lock_entry()});
        entries["../x"] = lock_entry();
        tree.write_skills_lock(entries);
        let outside = tree.write(
            "# x\n",
            &tree.paths.skills_cli_dir().join("x").join("SKILL.md"),
        );
        let lock_before = std::fs::read(tree.paths.skills_cli_lock()).expect("read lock");
        let git = Arc::new(source_probe());
        let trasher = Arc::new(FakeTrasher::new());
        let fettlr = fettlr_for(&tree, git.clone(), trasher.clone());
        fettlr.refresh().expect("refresh");

        let skill = hand_built("../x", tree.paths.skills_dir().join("x"));
        assert!(matches!(
            fettlr.pull_skill(&skill),
            Err(CoreError::UnsupportedSkillSource { .. })
        ));
        assert_eq!(git.fetch_count(URL), 0);
        assert!(git.exports().is_empty());
        assert!(trasher.trashed().is_empty());
        assert_eq!(std::fs::read_to_string(&outside).expect("read x"), "# x\n");
        assert_eq!(
            std::fs::read(tree.paths.skills_cli_lock()).expect("read lock"),
            lock_before
        );
    }

    #[test]
    fn refresh_restores_a_skill_left_set_aside_by_an_interrupted_update() {
        let tree = installed_tree();
        let canonical = tree.paths.skills_cli_skills_dir().join("animate");
        let set_aside = set_aside_folder(&tree);
        tree.make_directory(set_aside.parent().expect("parent"));
        std::fs::rename(&canonical, &set_aside).expect("set aside");
        let fettlr = fettlr_for(
            &tree,
            Arc::new(source_probe()),
            Arc::new(FakeTrasher::new()),
        );

        let snapshot = fettlr.refresh().expect("refresh");

        let skill = snapshot
            .skills
            .iter()
            .find(|skill| skill.name == "animate")
            .expect("animate is listed again");
        assert!(matches!(skill.provenance, Provenance::SkillsCli(_)));
        assert_eq!(
            std::fs::read_to_string(canonical_skill_md(&tree)).expect("read skill"),
            "# animate\n"
        );
        assert!(!set_aside.exists());
    }

    #[test]
    fn refresh_restores_a_skill_left_set_aside_under_the_older_name() {
        let tree = installed_tree();
        let canonical = tree.paths.skills_cli_skills_dir().join("animate");
        let legacy = tree.paths.skills_cli_dir().join(".fettlr-replaced-animate");
        std::fs::rename(&canonical, &legacy).expect("set aside");
        let fettlr = fettlr_for(
            &tree,
            Arc::new(source_probe()),
            Arc::new(FakeTrasher::new()),
        );

        fettlr.refresh().expect("refresh");

        assert_eq!(
            std::fs::read_to_string(canonical_skill_md(&tree)).expect("read skill"),
            "# animate\n"
        );
        assert!(!legacy.exists());
    }

    #[test]
    fn refresh_never_restores_through_a_linked_set_aside_folder() {
        let tree = installed_tree();
        let canonical = tree.paths.skills_cli_skills_dir().join("animate");
        let elsewhere = tree.root().join("elsewhere");
        tree.make_directory(&elsewhere);
        std::fs::rename(&canonical, elsewhere.join("animate")).expect("move away");
        std::os::unix::fs::symlink(
            &elsewhere,
            tree.paths.skills_cli_dir().join(".fettlr-replaced"),
        )
        .expect("link set-aside folder");
        let fettlr = fettlr_for(
            &tree,
            Arc::new(source_probe()),
            Arc::new(FakeTrasher::new()),
        );

        fettlr.refresh().expect("refresh");

        assert!(std::fs::symlink_metadata(&canonical).is_err());
        assert!(elsewhere.join("animate").join("SKILL.md").is_file());
    }

    #[test]
    fn refresh_leaves_a_set_aside_folder_alone_while_the_skill_folder_exists() {
        let tree = installed_tree();
        let set_aside = set_aside_folder(&tree);
        let old = tree.write("old", &set_aside.join("SKILL.md"));
        let fettlr = fettlr_for(
            &tree,
            Arc::new(source_probe()),
            Arc::new(FakeTrasher::new()),
        );

        fettlr.refresh().expect("refresh");

        assert_eq!(std::fs::read_to_string(&old).expect("read old"), "old");
        assert_eq!(
            std::fs::read_to_string(canonical_skill_md(&tree)).expect("read skill"),
            "# animate\n"
        );
    }
}
