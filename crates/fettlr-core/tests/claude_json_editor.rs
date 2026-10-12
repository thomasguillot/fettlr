use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use fettlr_core::testing::TempTree;
use fettlr_core::{CoreError, set_project_mcpjson_server};
use serde_json::{Map, Value, json};

struct Sample {
    tree: TempTree,
    project: String,
    other: String,
    original: Value,
}

fn make_sample(enabled: Option<Vec<&str>>, disabled: Option<Vec<&str>>) -> Sample {
    let tree = TempTree::new();
    let project = tree.make_project("example-project").display().to_string();
    let other = tree.make_project("other-project").display().to_string();

    let mut entry = json!({
        "lastCost": 1.25,
        "allowedTools": ["Bash(git status)"],
        "history": [{"display": "hello", "pastedContents": {}}],
        "mcpServers": {"playwright": {"command": "npx", "args": ["@playwright/mcp"]}},
        "hasTrustDialogAccepted": true,
    });
    if let Some(enabled) = enabled {
        entry["enabledMcpjsonServers"] = json!(enabled);
    }
    if let Some(disabled) = disabled {
        entry["disabledMcpjsonServers"] = json!(disabled);
    }

    let mut projects = Map::new();
    projects.insert(project.clone(), entry);
    projects.insert(
        other.clone(),
        json!({
            "lastCost": 0.5,
            "enabledMcpjsonServers": ["shared"],
            "disabledMcpjsonServers": [],
        }),
    );

    tree.write_claude_json(json!({
        "numStartups": 42,
        "installMethod": "native",
        "oauthAccount": {"emailAddress": "dev@example.com", "organizationRole": "admin"},
        "mcpServers": {"context7": {"command": "npx", "args": ["-y", "@upstash/context7-mcp"]}},
        "tipsHistory": {"new-user-warmup": 3},
        "projects": projects,
    }));

    let original = root(&tree);
    Sample {
        tree,
        project,
        other,
        original,
    }
}

fn root(tree: &TempTree) -> Value {
    let raw = std::fs::read_to_string(&tree.paths.claude_json).expect("read claude.json");
    serde_json::from_str(&raw).expect("decode claude.json")
}

fn project_entry<'a>(root: &'a Value, path: &str) -> &'a Value {
    root.get("projects")
        .and_then(|projects| projects.get(path))
        .unwrap_or_else(|| panic!("no project entry for {path}"))
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn set(sample: &Sample, name: &str, enabled: bool) -> Result<(), CoreError> {
    set_project_mcpjson_server(
        &sample.tree.paths.claude_json,
        &sample.project,
        name,
        enabled,
    )
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn disabling_moves_the_name_out_of_the_enabled_array() {
    let sample = make_sample(Some(vec!["figma", "notion"]), Some(vec!["legacy"]));

    set(&sample, "figma", false).expect("rewrites the arrays");

    let updated = root(&sample.tree);
    let entry = project_entry(&updated, &sample.project);
    assert_eq!(strings(entry.get("enabledMcpjsonServers")), ["notion"]);
    assert_eq!(
        strings(entry.get("disabledMcpjsonServers")),
        ["legacy", "figma"]
    );
}

#[test]
fn enabling_moves_the_name_out_of_the_disabled_array() {
    let sample = make_sample(Some(vec!["figma", "notion"]), Some(vec!["legacy"]));

    set(&sample, "legacy", true).expect("rewrites the arrays");

    let updated = root(&sample.tree);
    let entry = project_entry(&updated, &sample.project);
    assert_eq!(
        strings(entry.get("enabledMcpjsonServers")),
        ["figma", "notion", "legacy"]
    );
    assert!(strings(entry.get("disabledMcpjsonServers")).is_empty());
}

#[test]
fn a_name_in_neither_array_is_added_to_the_target_array() {
    let sample = make_sample(Some(vec!["figma", "notion"]), Some(vec!["legacy"]));

    set(&sample, "fresh", false).expect("rewrites the arrays");

    let updated = root(&sample.tree);
    let entry = project_entry(&updated, &sample.project);
    assert_eq!(
        strings(entry.get("enabledMcpjsonServers")),
        ["figma", "notion"]
    );
    assert_eq!(
        strings(entry.get("disabledMcpjsonServers")),
        ["legacy", "fresh"]
    );
}

#[test]
fn repeating_the_same_change_does_not_duplicate_the_name() {
    let sample = make_sample(Some(vec!["figma", "notion"]), Some(vec!["legacy"]));

    set(&sample, "legacy", false).expect("rewrites the arrays");
    set(&sample, "legacy", false).expect("rewrites the arrays again");

    let updated = root(&sample.tree);
    let entry = project_entry(&updated, &sample.project);
    assert_eq!(strings(entry.get("disabledMcpjsonServers")), ["legacy"]);
    assert_eq!(
        strings(entry.get("enabledMcpjsonServers")),
        ["figma", "notion"]
    );
}

#[test]
fn everything_else_in_the_file_survives() {
    let sample = make_sample(Some(vec!["figma", "notion"]), Some(vec!["legacy"]));
    let path = sample.tree.paths.claude_json.clone();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod 0600");

    set(&sample, "figma", false).expect("rewrites the arrays");

    assert_eq!(mode_of(&path), 0o600);
    let updated = root(&sample.tree);
    for (key, value) in sample.original.as_object().expect("object") {
        if key == "projects" {
            continue;
        }
        assert_eq!(updated.get(key), Some(value), "top-level key {key} changed");
    }
    assert_eq!(
        project_entry(&updated, &sample.other),
        project_entry(&sample.original, &sample.other)
    );

    let before = project_entry(&sample.original, &sample.project)
        .as_object()
        .expect("object");
    let after = project_entry(&updated, &sample.project)
        .as_object()
        .expect("object");
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>()
    );
    for (key, value) in before {
        if key == "enabledMcpjsonServers" || key == "disabledMcpjsonServers" {
            continue;
        }
        assert_eq!(after.get(key), Some(value), "project key {key} changed");
    }
}

#[test]
fn a_project_without_the_arrays_gains_only_the_one_it_needs() {
    let sample = make_sample(None, None);

    set(&sample, "figma", false).expect("rewrites the arrays");

    let updated = root(&sample.tree);
    let entry = project_entry(&updated, &sample.project);
    assert_eq!(strings(entry.get("disabledMcpjsonServers")), ["figma"]);
    assert_eq!(entry.get("enabledMcpjsonServers"), None);
    assert_eq!(
        entry.get("mcpServers"),
        project_entry(&sample.original, &sample.project).get("mcpServers")
    );
    assert_eq!(entry.get("lastCost"), Some(&json!(1.25)));
}

#[test]
fn an_absent_project_gains_an_entry() {
    let sample = make_sample(Some(vec!["figma", "notion"]), Some(vec!["legacy"]));
    let absent = sample
        .tree
        .absent_project("gone-project")
        .display()
        .to_string();

    set_project_mcpjson_server(&sample.tree.paths.claude_json, &absent, "figma", false)
        .expect("rewrites the arrays");

    let updated = root(&sample.tree);
    assert_eq!(
        strings(project_entry(&updated, &absent).get("disabledMcpjsonServers")),
        ["figma"]
    );
    assert_eq!(
        project_entry(&updated, &sample.project),
        project_entry(&sample.original, &sample.project)
    );
}

#[test]
fn a_malformed_file_is_refused_and_left_untouched() {
    let tree = TempTree::new();
    let garbage = "{ not json at all";
    tree.write_claude_json_raw(garbage);

    let error = set_project_mcpjson_server(&tree.paths.claude_json, "/anywhere", "figma", false)
        .expect_err("refuses a malformed file");

    match error {
        CoreError::MalformedRootConfig { path } => {
            assert_eq!(path, tree.paths.claude_json.display().to_string());
        }
        other => panic!("expected a malformed-root-config error, got {other:?}"),
    }
    assert_eq!(
        std::fs::read_to_string(&tree.paths.claude_json).expect("read"),
        garbage
    );
}

#[test]
fn a_projects_key_of_the_wrong_type_is_refused() {
    let tree = TempTree::new();
    let raw = r#"{"numStartups":42,"projects":"not-a-map"}"#;
    tree.write_claude_json_raw(raw);

    let error = set_project_mcpjson_server(&tree.paths.claude_json, "/anywhere", "figma", false)
        .expect_err("refuses a wrong-typed projects key");

    assert!(matches!(error, CoreError::MalformedRootConfig { .. }));
    assert_eq!(
        std::fs::read_to_string(&tree.paths.claude_json).expect("read"),
        raw
    );
}

#[test]
fn a_project_entry_of_the_wrong_type_is_refused() {
    let tree = TempTree::new();
    let raw = r#"{"projects":{"/anywhere":["not-a-map"]}}"#;
    tree.write_claude_json_raw(raw);

    let error = set_project_mcpjson_server(&tree.paths.claude_json, "/anywhere", "figma", false)
        .expect_err("refuses a wrong-typed project entry");

    assert!(matches!(error, CoreError::MalformedRootConfig { .. }));
    assert_eq!(
        std::fs::read_to_string(&tree.paths.claude_json).expect("read"),
        raw
    );
}

#[test]
fn a_file_without_a_projects_key_gains_one() {
    let tree = TempTree::new();
    tree.write_claude_json_raw(r#"{"numStartups":42}"#);

    set_project_mcpjson_server(&tree.paths.claude_json, "/anywhere", "figma", false)
        .expect("rewrites the arrays");

    let updated = root(&tree);
    assert_eq!(updated.get("numStartups"), Some(&json!(42)));
    assert_eq!(
        strings(project_entry(&updated, "/anywhere").get("disabledMcpjsonServers")),
        ["figma"]
    );
}

#[test]
fn a_missing_file_is_refused() {
    let tree = TempTree::new();

    let error = set_project_mcpjson_server(&tree.paths.claude_json, "/anywhere", "figma", false)
        .expect_err("refuses a missing file");

    match error {
        CoreError::UnreadableRootConfig { path } => {
            assert_eq!(path, tree.paths.claude_json.display().to_string());
        }
        other => panic!("expected an unreadable-root-config error, got {other:?}"),
    }
    assert!(!tree.paths.claude_json.exists());
}

#[test]
fn a_symlinked_config_stays_a_symlink() {
    let sample = make_sample(Some(vec!["figma"]), None);
    let real = sample
        .tree
        .paths
        .claude_json
        .parent()
        .expect("parent")
        .join("dotfiles.json");
    std::fs::rename(&sample.tree.paths.claude_json, &real).expect("move the real file aside");
    std::os::unix::fs::symlink(&real, &sample.tree.paths.claude_json).expect("symlink it back");

    set(&sample, "figma", false).expect("writes through the link");

    assert!(
        std::fs::symlink_metadata(&sample.tree.paths.claude_json)
            .expect("metadata")
            .file_type()
            .is_symlink(),
        "the rename replaced the link instead of writing through it"
    );
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(&real).expect("read the target"))
            .expect("decode");
    assert_eq!(
        strings(project_entry(&written, &sample.project).get("disabledMcpjsonServers")),
        ["figma"]
    );
}

#[test]
fn the_order_claude_code_wrote_the_file_in_survives() {
    let tree = TempTree::new();
    // Deliberately not alphabetical: this is the shape Claude Code actually writes.
    tree.write_claude_json(json!({
        "numStartups": 42,
        "installMethod": "brew",
        "autoUpdates": true,
        "projects": {},
    }));

    set_project_mcpjson_server(&tree.paths.claude_json, "/anywhere", "figma", false)
        .expect("writes");

    let raw = std::fs::read_to_string(&tree.paths.claude_json).expect("read");
    let root: Value = serde_json::from_str(&raw).expect("decode");
    let keys: Vec<&str> = root
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["numStartups", "installMethod", "autoUpdates", "projects"],
        "the file was reshuffled, making any diff or backup comparison against it useless"
    );
}

#[test]
fn a_disable_array_of_the_wrong_type_is_refused_and_left_untouched() {
    let tree = TempTree::new();
    let mut projects = Map::new();
    projects.insert(
        "/anywhere".to_string(),
        json!({"lastCost": 1.25, "disabledMcpjsonServers": "legacy"}),
    );
    tree.write_claude_json(json!({"projects": projects}));
    let before = std::fs::read_to_string(&tree.paths.claude_json).expect("read");

    let error = set_project_mcpjson_server(&tree.paths.claude_json, "/anywhere", "figma", false)
        .expect_err("refuses a wrong-typed array");

    assert!(matches!(error, CoreError::MalformedRootConfig { .. }));
    assert_eq!(
        std::fs::read_to_string(&tree.paths.claude_json).expect("read"),
        before,
        "the file was rewritten despite the refusal"
    );
}
