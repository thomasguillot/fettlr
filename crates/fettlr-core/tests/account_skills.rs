use fettlr_core::testing::TempTree;
use fettlr_core::{Provenance, Scope, discover};
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
    assert!(
        config
            .warnings
            .iter()
            .any(|warning| warning.contains(&manifest.display().to_string()))
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
