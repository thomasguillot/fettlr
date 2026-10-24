use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use fettlr_core::testing::TempTree;
use fettlr_core::{AccountSkillBehind, Provenance, Scope, Skill, account_skills_behind, discover};
use serde_json::{Value, json};

const UPDATED: &str = "2026-10-24T12:57:30.617177Z";

fn impeccable_entry() -> Value {
    json!({
        "skillId": "skill_01M9y7cQGGLWD3fn6H2hiZot",
        "name": "impeccable",
        "source": "plugin",
        "updatedAt": UPDATED,
        "backingPluginId": "plugin_014FSx7w2JGPq2xa75LSd5eJ"
    })
}

fn entry(name: &str, extra: Value) -> Value {
    let mut base = json!({"skillId": name, "name": name, "updatedAt": UPDATED});
    for (key, value) in extra.as_object().expect("object") {
        base[key] = value.clone();
    }
    base
}

#[test]
fn an_uploaded_skill_is_listed_as_an_account_skill() {
    let tree = TempTree::new();
    let bucket = tree.write_account_manifest("b1", 1, json!([impeccable_entry()]));
    let folder = tree.make_account_skill(
        "b1",
        "impeccable",
        &TempTree::skill_markdown("impeccable", "Design skill"),
    );

    let config = discover(&tree.paths).expect("discover");

    assert_eq!(config.account_skills.len(), 1);
    let skill = &config.account_skills[0];
    assert_eq!(skill.id, "account:impeccable");
    assert_eq!(skill.name, "impeccable");
    assert_eq!(skill.scope, Scope::User);
    assert_eq!(
        skill.provenance,
        Provenance::Account {
            updated_at: UPDATED.into()
        }
    );
    assert_eq!(skill.title.as_deref(), Some("impeccable"));
    assert_eq!(skill.summary.as_deref(), Some("Design skill"));
    assert_eq!(skill.path, folder);
    assert_eq!(skill.path, bucket.join("impeccable"));
}

#[test]
fn anthropic_skills_are_left_out() {
    let tree = TempTree::new();
    tree.write_account_manifest(
        "b1",
        1,
        json!([
            entry("pdf", json!({"source": "anthropic"})),
            entry("docs", json!({"source": "anthropic-example"})),
            entry(
                "morning",
                json!({"source": "plugin", "creatorType": "anthropic"})
            ),
        ]),
    );
    for name in ["pdf", "docs", "morning"] {
        tree.make_account_skill("b1", name, &TempTree::skill_markdown(name, "x"));
    }

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
}

#[test]
fn an_entry_whose_folder_has_no_skill_md_is_skipped() {
    let tree = TempTree::new();
    let bucket = tree.write_account_manifest("b1", 1, json!([impeccable_entry()]));
    tree.write("notes", &bucket.join("impeccable").join("notes.md"));

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
}

#[test]
fn an_entry_with_a_path_like_name_is_skipped() {
    let tree = TempTree::new();
    tree.write_account_manifest(
        "b1",
        1,
        json!([
            entry("../escape", json!({"source": "plugin"})),
            entry(".hidden", json!({"source": "plugin"})),
        ]),
    );
    tree.make_account_skill("b1", ".hidden", &TempTree::skill_markdown("hidden", "x"));
    tree.write(
        &TempTree::skill_markdown("escape", "x"),
        &tree.paths.account_skills_dir().join("escape/SKILL.md"),
    );

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
}

#[test]
fn sync_switched_off_lists_nothing() {
    let tree = TempTree::new();
    tree.write_account_manifest("b1", 1, json!([impeccable_entry()]));
    tree.make_account_skill(
        "b1",
        "impeccable",
        &TempTree::skill_markdown("impeccable", "Design skill"),
    );
    tree.write(
        r#"{"syncClaudeAiSkills": false}"#,
        &tree.paths.settings_json(),
    );

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
    assert!(config.warnings.is_empty());
}

#[test]
fn a_malformed_manifest_warns_and_lists_nothing() {
    let tree = TempTree::new();
    let bucket = tree.write_account_manifest("b1", 1, json!([]));
    let manifest = bucket.join("manifest.json");
    tree.write("{", &manifest);

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
    assert_eq!(
        config.warnings,
        [format!(
            "Could not read the Claude account skills listed in {}.",
            manifest.display()
        )]
    );
}

#[test]
fn the_most_recently_synced_bucket_wins() {
    let tree = TempTree::new();
    tree.write_account_manifest("a", 1, json!([entry("old", json!({"source": "plugin"}))]));
    tree.write_account_manifest("b", 2, json!([entry("new", json!({"source": "plugin"}))]));
    tree.make_account_skill("a", "old", &TempTree::skill_markdown("old", "x"));
    tree.make_account_skill("b", "new", &TempTree::skill_markdown("new", "x"));

    let config = discover(&tree.paths).expect("discover");

    let ids: Vec<_> = config
        .account_skills
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(ids, ["account:new"]);
}

#[test]
fn no_sync_folder_lists_nothing() {
    let tree = TempTree::new();

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
    assert!(config.warnings.is_empty());
}

#[test]
fn the_manifest_description_fills_a_missing_summary() {
    let tree = TempTree::new();
    tree.write_account_manifest(
        "b1",
        1,
        json!([entry(
            "plain",
            json!({"source": "plugin", "description": "From the manifest"})
        )]),
    );
    tree.make_account_skill("b1", "plain", "---\nname: plain\n---\n\n# plain\n");

    let config = discover(&tree.paths).expect("discover");

    assert_eq!(
        config.account_skills[0].summary.as_deref(),
        Some("From the manifest")
    );
}

fn plugin_entry(name: &str) -> Value {
    entry(name, json!({"source": "plugin"}))
}

#[test]
fn a_symlinked_skill_folder_is_skipped() {
    let tree = TempTree::new();
    let bucket = tree.write_account_manifest("b1", 1, json!([plugin_entry("linked")]));
    let target = tree.make_directory(&tree.root().join("elsewhere"));
    tree.write(
        &TempTree::skill_markdown("linked", "x"),
        &target.join("SKILL.md"),
    );
    std::os::unix::fs::symlink(&target, bucket.join("linked")).expect("link");

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
}

#[test]
fn a_symlinked_skill_md_is_skipped() {
    let tree = TempTree::new();
    let bucket = tree.write_account_manifest("b1", 1, json!([plugin_entry("linked")]));
    let real = tree.write(
        &TempTree::skill_markdown("linked", "x"),
        &tree.root().join("real.md"),
    );
    tree.make_directory(&bucket.join("linked"));
    std::os::unix::fs::symlink(&real, bucket.join("linked").join("SKILL.md")).expect("link");

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
}

#[test]
fn a_symlinked_bucket_is_not_used() {
    let tree = TempTree::new();
    let real = tree.root().join("real-bucket");
    tree.write(
        &serde_json::to_string(&json!({"lastUpdated": 1, "skills": [plugin_entry("inside")]}))
            .unwrap(),
        &real.join("manifest.json"),
    );
    tree.write(
        &TempTree::skill_markdown("inside", "x"),
        &real.join("inside/SKILL.md"),
    );
    tree.make_directory(&tree.paths.account_skills_dir());
    std::os::unix::fs::symlink(&real, tree.paths.account_skills_dir().join("b1")).expect("link");

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
}

#[test]
fn a_symlinked_manifest_is_not_used() {
    let tree = TempTree::new();
    let bucket = tree.write_account_manifest("b1", 1, json!([]));
    let real = tree.write(
        &serde_json::to_string(&json!({"lastUpdated": 1, "skills": [plugin_entry("inside")]}))
            .unwrap(),
        &tree.root().join("manifest.json"),
    );
    std::fs::remove_file(bucket.join("manifest.json")).unwrap();
    std::os::unix::fs::symlink(&real, bucket.join("manifest.json")).expect("link");
    tree.make_account_skill("b1", "inside", &TempTree::skill_markdown("inside", "x"));

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
}

#[test]
fn equal_last_updated_goes_to_the_bucket_that_sorts_first() {
    let tree = TempTree::new();
    tree.write_account_manifest("b", 5, json!([plugin_entry("second")]));
    tree.write_account_manifest("a", 5, json!([plugin_entry("first")]));
    tree.make_account_skill("a", "first", &TempTree::skill_markdown("first", "x"));
    tree.make_account_skill("b", "second", &TempTree::skill_markdown("second", "x"));

    let config = discover(&tree.paths).expect("discover");

    let ids: Vec<_> = config
        .account_skills
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(ids, ["account:first"]);
}

#[test]
fn a_readable_bucket_beats_a_malformed_one_that_sorts_first() {
    let tree = TempTree::new();
    let bad = tree.write_account_manifest("a", 9, json!([]));
    tree.write("{", &bad.join("manifest.json"));
    tree.write_account_manifest("b", 0, json!([plugin_entry("good")]));
    tree.make_account_skill("b", "good", &TempTree::skill_markdown("good", "x"));

    let config = discover(&tree.paths).expect("discover");

    let ids: Vec<_> = config
        .account_skills
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(ids, ["account:good"]);
    assert!(config.warnings.is_empty());
}

#[test]
fn a_fractional_last_updated_still_ranks() {
    let tree = TempTree::new();
    tree.write_account_manifest("a", 0, json!([plugin_entry("old")]));
    let newer = tree.write_account_manifest("b", 0, json!([]));
    tree.write(
        &serde_json::to_string(&json!({"lastUpdated": 2.5, "skills": [plugin_entry("new")]}))
            .unwrap(),
        &newer.join("manifest.json"),
    );
    tree.make_account_skill("a", "old", &TempTree::skill_markdown("old", "x"));
    tree.make_account_skill("b", "new", &TempTree::skill_markdown("new", "x"));

    let config = discover(&tree.paths).expect("discover");

    let ids: Vec<_> = config
        .account_skills
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(ids, ["account:new"]);
}

#[test]
fn a_duplicate_name_keeps_the_first_entry() {
    let tree = TempTree::new();
    tree.write_account_manifest(
        "b1",
        1,
        json!([
            entry("dup", json!({"source": "plugin", "updatedAt": "first"})),
            entry("dup", json!({"source": "plugin", "updatedAt": "second"})),
        ]),
    );
    tree.make_account_skill("b1", "dup", &TempTree::skill_markdown("dup", "x"));

    let config = discover(&tree.paths).expect("discover");

    assert_eq!(config.account_skills.len(), 1);
    assert_eq!(
        config.account_skills[0].provenance,
        Provenance::Account {
            updated_at: "first".into()
        }
    );
}

#[test]
fn a_manifest_of_the_wrong_shape_warns_and_lists_nothing() {
    for raw in ["[]", r#"{"skills": "nope"}"#, r#"{"lastUpdated": 1}"#] {
        let tree = TempTree::new();
        let bucket = tree.write_account_manifest("b1", 1, json!([]));
        let manifest = bucket.join("manifest.json");
        tree.write(raw, &manifest);

        let config = discover(&tree.paths).expect("discover");

        assert!(config.account_skills.is_empty(), "{raw}");
        assert_eq!(
            config.warnings,
            [format!(
                "Could not read the Claude account skills listed in {}.",
                manifest.display()
            )],
            "{raw}"
        );
    }
}

#[test]
fn a_string_false_leaves_sync_on() {
    let tree = TempTree::new();
    tree.write_account_manifest("b1", 1, json!([plugin_entry("kept")]));
    tree.make_account_skill("b1", "kept", &TempTree::skill_markdown("kept", "x"));
    tree.write(
        r#"{"syncClaudeAiSkills": "false"}"#,
        &tree.paths.settings_json(),
    );

    let config = discover(&tree.paths).expect("discover");

    assert_eq!(config.account_skills.len(), 1);
}

#[test]
fn a_name_with_a_backslash_is_skipped() {
    let tree = TempTree::new();
    tree.write_account_manifest("b1", 1, json!([plugin_entry("a\\b")]));
    tree.make_account_skill("b1", "a\\b", &TempTree::skill_markdown("ab", "x"));

    let config = discover(&tree.paths).expect("discover");

    assert!(config.account_skills.is_empty());
}

const PLUGIN_ID: &str = "plugin:impeccable@impeccable/impeccable";
const UPLOADED: i64 = 1_792_846_650;

fn markdown(version: Option<&str>, indent_tools: bool, body: &str) -> String {
    let version = version
        .map(|v| format!("version: {v}\n"))
        .unwrap_or_default();
    let tools = if indent_tools {
        "allowed-tools:\n  - Bash(x)\n"
    } else {
        "allowed-tools:\n- Bash(x)\n"
    };
    format!("---\nname: impeccable\ndescription: d\n{version}{tools}---\n\n{body}\n")
}

fn account(tree: &TempTree, markdown: &str, updated_at: &str) -> Skill {
    let folder = tree.make_account_skill("b1", "impeccable", markdown);
    Skill {
        id: "account:impeccable".into(),
        name: "impeccable".into(),
        title: Some("impeccable".into()),
        summary: None,
        scope: Scope::User,
        provenance: Provenance::Account {
            updated_at: updated_at.into(),
        },
        path: folder,
    }
}

fn local(
    tree: &TempTree,
    dir: &str,
    id: &str,
    scope: Scope,
    provenance: Provenance,
    markdown: &str,
) -> Skill {
    let folder = tree.root().join(dir).join("impeccable");
    tree.write(markdown, &folder.join("SKILL.md"));
    Skill {
        id: id.into(),
        name: "impeccable".into(),
        title: Some("impeccable".into()),
        summary: None,
        scope,
        provenance,
        path: folder,
    }
}

fn plugin_copy(tree: &TempTree, markdown: &str) -> Skill {
    local(
        tree,
        "plugin",
        PLUGIN_ID,
        Scope::User,
        Provenance::Plugin {
            plugin_id: "impeccable@impeccable".into(),
        },
        markdown,
    )
}

fn user_copy(tree: &TempTree, markdown: &str) -> Skill {
    local(
        tree,
        "user",
        "user/impeccable",
        Scope::User,
        Provenance::Local,
        markdown,
    )
}

fn set_mtime(path: &Path, unix_seconds: u64) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).expect("read dir") {
            set_mtime(&entry.expect("entry").path(), unix_seconds);
        }
    } else {
        std::fs::File::options()
            .write(true)
            .open(path)
            .expect("open")
            .set_modified(UNIX_EPOCH + Duration::from_secs(unix_seconds))
            .expect("set mtime");
    }
}

fn behind(account: &Skill, local: &Skill, version: Option<&str>) -> Vec<AccountSkillBehind> {
    vec![AccountSkillBehind {
        account_id: account.id.clone(),
        local_id: local.id.clone(),
        local_version: version.map(str::to_string),
    }]
}

fn judge(skills: &[&Skill]) -> Vec<AccountSkillBehind> {
    let owned: Vec<Skill> = skills.iter().map(|s| (*s).clone()).collect();
    account_skills_behind(&owned)
}

fn future_updated_at() -> String {
    "2099-01-01T00:00:00Z".into()
}

#[test]
fn a_versioned_local_copy_beats_an_unversioned_account_copy() {
    let tree = TempTree::new();
    let a = account(
        &tree,
        &markdown(None, false, "old body"),
        &future_updated_at(),
    );
    let c = plugin_copy(&tree, &markdown(Some("4.7.1"), true, "new body"));

    assert_eq!(judge(&[&a, &c]), behind(&a, &c, Some("4.7.1")));
}

#[test]
fn the_same_content_is_never_behind() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "body"), UPDATED);
    let c = plugin_copy(&tree, &markdown(None, true, "body"));
    set_mtime(&c.path, UPLOADED as u64 + 86_400);

    assert!(judge(&[&a, &c]).is_empty());
}

#[test]
fn a_server_dropped_version_key_does_not_make_a_copy_differ() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "body"), UPDATED);
    let c = plugin_copy(&tree, &markdown(Some("4.7.1"), true, "body"));

    assert!(judge(&[&a, &c]).is_empty());
}

#[test]
fn an_older_account_version_is_behind() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(Some("4.6.0"), false, "old"), UPDATED);
    let c = plugin_copy(&tree, &markdown(Some("4.7.1"), true, "new"));

    assert_eq!(judge(&[&a, &c]), behind(&a, &c, Some("4.7.1")));
}

#[test]
fn an_equal_or_newer_account_version_is_not_behind() {
    for account_version in ["4.7.1", "4.8.0"] {
        let tree = TempTree::new();
        let a = account(
            &tree,
            &markdown(Some(account_version), false, "old"),
            UPDATED,
        );
        let c = plugin_copy(&tree, &markdown(Some("4.7.1"), true, "new"));

        assert!(judge(&[&a, &c]).is_empty(), "{account_version}");
    }
}

#[test]
fn uncomparable_versions_are_not_behind() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), UPDATED);
    let c = plugin_copy(&tree, &markdown(Some("next"), true, "new"));

    assert!(judge(&[&a, &c]).is_empty());
}

#[test]
fn an_unversioned_local_copy_edited_after_the_upload_is_behind() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), "2026-01-01T00:00:00Z");
    let c = user_copy(&tree, &markdown(None, true, "new"));
    set_mtime(&c.path, UPLOADED as u64);

    assert_eq!(judge(&[&a, &c]), behind(&a, &c, None));
}

#[test]
fn an_unversioned_local_copy_older_than_the_upload_is_not_behind() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), "2026-10-24T12:57:30Z");
    let c = user_copy(&tree, &markdown(None, true, "new"));
    set_mtime(&c.path, UPLOADED as u64 - 86_400);

    assert!(judge(&[&a, &c]).is_empty());
}

#[test]
fn a_local_edit_within_five_minutes_of_the_upload_is_not_trusted() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), "2026-10-24T12:57:30Z");
    let c = user_copy(&tree, &markdown(None, true, "new"));
    set_mtime(&c.path, UPLOADED as u64 + 299);

    assert!(judge(&[&a, &c]).is_empty());
}

#[test]
fn the_edit_window_is_strictly_longer_than_five_minutes() {
    for (offset, expected_behind) in [(300, false), (301, true)] {
        let tree = TempTree::new();
        let a = account(&tree, &markdown(None, false, "old"), "2026-10-24T12:57:30Z");
        let c = user_copy(&tree, &markdown(None, true, "new"));
        set_mtime(&c.path, UPLOADED as u64 + offset);

        assert_eq!(
            !judge(&[&a, &c]).is_empty(),
            expected_behind,
            "{offset} seconds after the upload"
        );
    }
}

#[test]
fn the_newer_of_two_unversioned_copies_is_chosen() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), "2026-10-24T12:57:30Z");
    let plugin = plugin_copy(&tree, &markdown(None, true, "p"));
    let user = user_copy(&tree, &markdown(None, true, "u"));
    set_mtime(&plugin.path, UPLOADED as u64 + 1_000);
    set_mtime(&user.path, UPLOADED as u64 + 2_000);

    assert_eq!(judge(&[&a, &plugin, &user]), behind(&a, &user, None));
}

#[test]
fn equally_new_unversioned_copies_go_to_the_smaller_id() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), "2026-10-24T12:57:30Z");
    let plugin = plugin_copy(&tree, &markdown(None, true, "p"));
    let user = user_copy(&tree, &markdown(None, true, "u"));
    set_mtime(&plugin.path, UPLOADED as u64 + 1_000);
    set_mtime(&user.path, UPLOADED as u64 + 1_000);

    assert!(plugin.id < user.id);
    assert_eq!(judge(&[&a, &user, &plugin]), behind(&a, &plugin, None));
}

#[test]
fn a_copy_whose_frontmatter_name_differs_is_not_a_candidate() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), UPDATED);
    let mut renamed = plugin_copy(&tree, &markdown(Some("9.0.0"), true, "new"));
    renamed.title = Some("impeccable-pro".into());

    assert!(judge(&[&a, &renamed]).is_empty());
}

#[test]
fn a_copy_in_another_folder_with_the_same_frontmatter_name_is_a_candidate() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), UPDATED);
    let mut other = plugin_copy(&tree, &markdown(Some("4.7.1"), true, "new"));
    other.name = "x".into();
    other.title = Some("impeccable".into());

    assert_eq!(judge(&[&a, &other]), behind(&a, &other, Some("4.7.1")));
}

#[test]
fn a_changed_description_makes_the_copy_differ() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "body"), UPDATED);
    let c = plugin_copy(
        &tree,
        &markdown(Some("4.7.1"), true, "body").replace("description: d", "description: changed"),
    );

    assert_eq!(judge(&[&a, &c]), behind(&a, &c, Some("4.7.1")));
}

#[test]
fn a_project_skill_is_never_compared() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), UPDATED);
    let c = local(
        &tree,
        "project",
        "project/impeccable",
        Scope::Project { path: "/p".into() },
        Provenance::Local,
        &markdown(Some("9.0.0"), true, "new"),
    );

    assert!(judge(&[&a, &c]).is_empty());
}

#[test]
fn a_matching_copy_anywhere_keeps_the_account_current() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "body"), UPDATED);
    let same = user_copy(&tree, &markdown(None, true, "body"));
    let newer = plugin_copy(&tree, &markdown(Some("4.7.1"), true, "other"));

    assert!(judge(&[&a, &same, &newer]).is_empty());
}

#[test]
fn the_highest_version_is_chosen_to_package() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), UPDATED);
    let plugin = plugin_copy(&tree, &markdown(Some("4.7.1"), true, "p"));
    let user = user_copy(&tree, &markdown(Some("4.8.0"), true, "u"));

    let result = judge(&[&a, &plugin, &user]);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].local_id, "user/impeccable");
    assert_eq!(result[0].local_version.as_deref(), Some("4.8.0"));
}

#[test]
fn dotfiles_and_symlinks_are_ignored_in_the_comparison() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "body"), UPDATED);
    let c = plugin_copy(&tree, &markdown(Some("4.7.1"), true, "body"));
    tree.write("junk", &c.path.join(".DS_Store"));
    tree.write("ref: x", &c.path.join(".git").join("HEAD"));
    let outside = tree.write("outside", &tree.root().join("outside.md"));
    std::os::unix::fs::symlink(&outside, c.path.join("extra.md")).expect("link");

    assert!(judge(&[&a, &c]).is_empty());
}

#[test]
fn a_file_only_one_side_has_makes_them_differ() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "body"), UPDATED);
    let c = plugin_copy(&tree, &markdown(Some("4.7.1"), true, "body"));
    tree.write("new", &c.path.join("reference").join("new.md"));

    assert_eq!(judge(&[&a, &c]), behind(&a, &c, Some("4.7.1")));
}

#[test]
fn a_skills_cli_skill_is_read_through_its_link() {
    let tree = TempTree::new();
    let a = account(&tree, &markdown(None, false, "old"), UPDATED);
    let link: PathBuf = tree.make_skills_cli_skill("impeccable");
    let canonical = tree.paths.skills_cli_skills_dir().join("impeccable");
    tree.write(
        &markdown(Some("2.0.0"), true, "new"),
        &canonical.join("SKILL.md"),
    );
    let c = Skill {
        id: "user/impeccable".into(),
        name: "impeccable".into(),
        title: None,
        summary: None,
        scope: Scope::User,
        provenance: Provenance::Local,
        path: link,
    };

    assert_eq!(judge(&[&a, &c]), behind(&a, &c, Some("2.0.0")));
}
