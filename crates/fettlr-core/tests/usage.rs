use std::path::PathBuf;

use fettlr_core::{Provenance, Scope, Skill, UsageIndex};
use serde_json::{Map, Value, json};

fn root_config() -> Map<String, Value> {
    let mut root = Map::new();
    root.insert(
        "skillUsage".to_string(),
        json!({
            "newspack:pr-create": {"usageCount": 102, "lastUsedAt": 1_787_415_121_723i64},
            "newspack-shots": {"usageCount": 88, "lastUsedAt": 1_787_411_501_395i64},
            "code-review": {"usageCount": 12, "lastUsedAt": 1_787_411_501_395i64},
            "mattpocock-skills:code-review": {"usageCount": 3},
            "ui-ux-pro-max:design": {"usageCount": 0},
        }),
    );
    root.insert(
        "pluginUsage".to_string(),
        json!({
            "newspack@newspack-devkit": {"usageCount": 21414, "lastUsedAt": 1_788_000_640_511i64},
            "linear@claude-plugins-official": {"usageCount": 0},
        }),
    );
    root
}

fn loose_skill(name: &str) -> Skill {
    Skill {
        id: format!("user/{name}"),
        name: name.to_string(),
        title: None,
        summary: None,
        scope: Scope::User,
        provenance: Provenance::Local,
        path: PathBuf::from("/tmp").join(name),
    }
}

fn plugin_skill(plugin_id: &str, name: &str) -> Skill {
    Skill {
        id: format!("plugin:{plugin_id}/{name}"),
        name: name.to_string(),
        title: None,
        summary: None,
        scope: Scope::User,
        provenance: Provenance::Plugin {
            plugin_id: plugin_id.to_string(),
        },
        path: PathBuf::from("/tmp").join(name),
    }
}

#[test]
fn a_loose_skill_joins_on_its_folder_name() {
    let index = UsageIndex::from_root(&root_config());
    let usage = index.for_skill(&loose_skill("newspack-shots")).unwrap();
    assert_eq!(usage.count, 88);
}

#[test]
fn a_plugin_skill_joins_on_the_bare_plugin_name() {
    // `claude plugin list --json` reports the id as name@marketplace, while skillUsage keys use
    // the bare name, so the marketplace has to be dropped before the lookup.
    let index = UsageIndex::from_root(&root_config());
    let usage = index
        .for_skill(&plugin_skill("newspack@newspack-devkit", "pr-create"))
        .unwrap();
    assert_eq!(usage.count, 102);
}

#[test]
fn a_bare_key_never_credits_a_namespaced_skill() {
    // ~/.claude.json carries both `code-review` and `mattpocock-skills:code-review`, and they are
    // different skills. Summing the two forms would credit each with the other's history.
    let index = UsageIndex::from_root(&root_config());
    assert_eq!(
        index.for_skill(&loose_skill("code-review")).unwrap().count,
        12
    );
    assert_eq!(
        index
            .for_skill(&plugin_skill("mattpocock-skills@official", "code-review"))
            .unwrap()
            .count,
        3
    );
}

#[test]
fn an_unrecorded_skill_reads_as_absent() {
    let index = UsageIndex::from_root(&root_config());
    assert!(index.for_skill(&loose_skill("never-run")).is_none());
}

#[test]
fn a_recorded_zero_is_kept_and_is_not_absence() {
    // The whole point of the feature: a skill that has never fired is a finding, while a skill
    // Claude Code has no record of is not. Collapsing both to zero would report the second as disuse.
    let index = UsageIndex::from_root(&root_config());
    let usage = index
        .for_skill(&plugin_skill("ui-ux-pro-max@whatever", "design"))
        .unwrap();
    assert_eq!(usage.count, 0);
    assert!(usage.last_used_at.is_none());
}

#[test]
fn last_used_at_converts_milliseconds_to_rfc3339() {
    let index = UsageIndex::from_root(&root_config());
    let usage = index.for_skill(&loose_skill("newspack-shots")).unwrap();
    assert_eq!(usage.last_used_at.as_deref(), Some("2026-08-22T15:11:41Z"));
}

#[test]
fn a_plugin_joins_on_its_full_id() {
    let index = UsageIndex::from_root(&root_config());
    let usage = index.for_plugin("newspack@newspack-devkit").unwrap();
    assert_eq!(usage.count, 21414);
    assert_eq!(usage.last_used_at.as_deref(), Some("2026-08-29T10:50:40Z"));
}

#[test]
fn an_unrecorded_plugin_reads_as_absent() {
    let index = UsageIndex::from_root(&root_config());
    assert!(index.for_plugin("ghost@nowhere").is_none());
}

#[test]
fn a_config_without_usage_maps_yields_an_empty_index() {
    let index = UsageIndex::from_root(&Map::new());
    assert!(index.for_skill(&loose_skill("newspack-shots")).is_none());
    assert!(index.for_plugin("newspack@newspack-devkit").is_none());
}

#[test]
fn a_malformed_usage_map_is_ignored_rather_than_failing() {
    let mut root = Map::new();
    root.insert("skillUsage".to_string(), json!("not an object"));
    root.insert("pluginUsage".to_string(), json!([1, 2, 3]));
    let index = UsageIndex::from_root(&root);
    assert!(index.for_skill(&loose_skill("newspack-shots")).is_none());
}

#[test]
fn an_entry_missing_its_count_is_skipped_rather_than_counted_as_zero() {
    let mut root = Map::new();
    root.insert(
        "skillUsage".to_string(),
        json!({"half-written": {"lastUsedAt": 1_787_411_501_395i64}}),
    );
    let index = UsageIndex::from_root(&root);
    assert!(index.for_skill(&loose_skill("half-written")).is_none());
}

#[test]
fn a_plugin_id_without_a_marketplace_still_resolves() {
    let mut root = Map::new();
    root.insert(
        "skillUsage".to_string(),
        json!({"local-kit:build": {"usageCount": 4}}),
    );
    let index = UsageIndex::from_root(&root);
    assert_eq!(
        index
            .for_skill(&plugin_skill("local-kit", "build"))
            .unwrap()
            .count,
        4
    );
}

#[test]
fn a_plugin_id_carrying_an_at_sign_splits_at_the_last_one() {
    // split_plugin_id in the core takes the name as everything before the final `@`; the usage key
    // has to be built the same way or the two disagree about what a plugin is called.
    let mut root = Map::new();
    root.insert(
        "skillUsage".to_string(),
        json!({"scope@team:review": {"usageCount": 9}}),
    );
    let index = UsageIndex::from_root(&root);
    assert_eq!(
        index
            .for_skill(&plugin_skill("scope@team@marketplace", "review"))
            .unwrap()
            .count,
        9
    );
}

#[test]
fn discover_carries_usage_off_the_root_config() {
    // Proves the wiring, not just the parser: discover() already reads ~/.claude.json for MCP
    // servers and projects, and the usage maps have to come off that same read.
    let tree = fettlr_core::testing::TempTree::new();
    tree.write_claude_json(json!({
        "mcpServers": {},
        "skillUsage": {"newspack-shots": {"usageCount": 88, "lastUsedAt": 1_787_411_501_395i64}},
        "pluginUsage": {"newspack@newspack-devkit": {"usageCount": 21414}},
    }));
    tree.make_skill("newspack-shots", &tree.user_skills_dir(), None, false);

    let config = fettlr_core::discover(&tree.paths).expect("discovery succeeds");
    let skill = config
        .user_skills
        .iter()
        .find(|candidate| candidate.name == "newspack-shots")
        .expect("the skill is discovered");

    assert_eq!(config.usage.for_skill(skill).unwrap().count, 88);
    assert_eq!(
        config
            .usage
            .for_plugin("newspack@newspack-devkit")
            .unwrap()
            .count,
        21414
    );
}

#[test]
fn a_claude_json_without_usage_leaves_discovery_working() {
    let tree = fettlr_core::testing::TempTree::new();
    tree.write_claude_json(json!({"mcpServers": {}}));
    tree.make_skill("newspack-shots", &tree.user_skills_dir(), None, false);

    let config = fettlr_core::discover(&tree.paths).expect("discovery succeeds");
    let skill = &config.user_skills[0];
    assert!(config.usage.for_skill(skill).is_none());
}
