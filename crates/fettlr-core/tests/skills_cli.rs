use fettlr_core::testing::{FakeTrasher, TempTree};
use fettlr_core::{
    CoreError, SkillsLock, read_skills_lock, record_skill_update, replace_skill_folder,
};
use serde_json::{Value, json};

const HASH: &str = "0123456789abcdef0123456789abcdef01234567";
const NEW_HASH: &str = "fedcba9876543210fedcba9876543210fedcba98";

fn entry(skill_path: &str) -> Value {
    json!({
        "source": "owner/repo",
        "sourceType": "github",
        "sourceUrl": "https://github.com/owner/repo.git",
        "skillPath": skill_path,
        "skillFolderHash": HASH,
        "installedAt": "2026-01-01T00:00:00.000Z",
        "updatedAt": "2026-01-02T00:00:00.000Z"
    })
}

#[test]
fn a_github_entry_decodes_its_folder_and_hash() {
    let tree = TempTree::new();
    tree.write_skills_lock(json!({"animate": entry("skills/animate/SKILL.md")}));
    let SkillsLock::Read(entries) = read_skills_lock(&tree.paths.skills_cli_lock()).unwrap() else {
        panic!("expected Read");
    };
    let origin = &entries["animate"];
    assert_eq!(origin.source, "owner/repo");
    assert_eq!(origin.source_type, "github");
    assert_eq!(
        origin.source_url.as_deref(),
        Some("https://github.com/owner/repo.git")
    );
    assert_eq!(origin.folder, "skills/animate");
    assert_eq!(origin.folder_hash, HASH);
    assert_eq!(origin.reference, None);
}

#[test]
fn a_ref_is_carried_as_the_reference() {
    let tree = TempTree::new();
    let mut with_ref = entry("skills/animate/SKILL.md");
    with_ref["ref"] = json!("next");
    tree.write_skills_lock(json!({"animate": with_ref}));
    let SkillsLock::Read(entries) = read_skills_lock(&tree.paths.skills_cli_lock()).unwrap() else {
        panic!("expected Read");
    };
    assert_eq!(entries["animate"].reference.as_deref(), Some("next"));
}

#[test]
fn a_root_skill_md_has_an_empty_folder() {
    let tree = TempTree::new();
    tree.write_skills_lock(json!({"solo": entry("SKILL.md")}));
    let SkillsLock::Read(entries) = read_skills_lock(&tree.paths.skills_cli_lock()).unwrap() else {
        panic!("expected Read");
    };
    assert_eq!(entries["solo"].folder, "");
}

#[test]
fn a_skill_md_beside_dot_slash_has_an_empty_folder() {
    let tree = TempTree::new();
    tree.write_skills_lock(json!({"solo": entry("./SKILL.md")}));
    let SkillsLock::Read(entries) = read_skills_lock(&tree.paths.skills_cli_lock()).unwrap() else {
        panic!("expected Read");
    };
    assert_eq!(entries["solo"].folder, "");
}

#[test]
fn an_empty_ref_is_no_reference() {
    let tree = TempTree::new();
    let mut with_ref = entry("skills/animate/SKILL.md");
    with_ref["ref"] = json!("");
    tree.write_skills_lock(json!({"animate": with_ref}));
    let SkillsLock::Read(entries) = read_skills_lock(&tree.paths.skills_cli_lock()).unwrap() else {
        panic!("expected Read");
    };
    assert_eq!(entries["animate"].reference, None);
}

#[test]
fn a_missing_lock_is_missing() {
    let tree = TempTree::new();
    assert!(matches!(
        read_skills_lock(&tree.paths.skills_cli_lock()).unwrap(),
        SkillsLock::Missing
    ));
}

#[test]
fn a_version_other_than_three_is_unsupported() {
    let tree = TempTree::new();
    tree.write(
        r#"{"version": 4, "skills": {}}"#,
        &tree.paths.skills_cli_lock(),
    );
    match read_skills_lock(&tree.paths.skills_cli_lock()).unwrap() {
        SkillsLock::Unsupported { version } => assert_eq!(version, "4"),
        _ => panic!("expected Unsupported"),
    }
}

#[test]
fn unparseable_json_is_a_bad_json_error() {
    let tree = TempTree::new();
    tree.write("{ nope", &tree.paths.skills_cli_lock());
    assert!(matches!(
        read_skills_lock(&tree.paths.skills_cli_lock()),
        Err(CoreError::BadJson(_))
    ));
    assert_eq!(
        std::fs::read_to_string(tree.paths.skills_cli_lock()).unwrap(),
        "{ nope"
    );
}

#[test]
fn an_entry_without_a_hash_is_skipped() {
    let tree = TempTree::new();
    let mut broken = entry("skills/broken/SKILL.md");
    broken.as_object_mut().unwrap().remove("skillFolderHash");
    tree.write_skills_lock(json!({
        "broken": broken,
        "animate": entry("skills/animate/SKILL.md")
    }));
    let SkillsLock::Read(entries) = read_skills_lock(&tree.paths.skills_cli_lock()).unwrap() else {
        panic!("expected Read");
    };
    assert!(!entries.contains_key("broken"));
    assert!(entries.contains_key("animate"));
}

#[test]
fn recording_an_update_changes_only_the_two_fields() {
    let tree = TempTree::new();
    tree.write_skills_lock(json!({
        "animate": entry("skills/animate/SKILL.md"),
        "other": entry("skills/other/SKILL.md")
    }));
    let path = tree.paths.skills_cli_lock();
    let before: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

    record_skill_update(&path, "animate", NEW_HASH, "2026-10-24T10:00:00Z").unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("{\n  \"version\": 3"));
    let after: Value = serde_json::from_str(&text).unwrap();
    let mut expected = before.clone();
    expected["skills"]["animate"]["skillFolderHash"] = json!(NEW_HASH);
    expected["skills"]["animate"]["updatedAt"] = json!("2026-10-24T10:00:00Z");
    assert_eq!(after, expected);
    assert_eq!(text, serde_json::to_string_pretty(&expected).unwrap());
    assert_eq!(
        after["skills"]["animate"]["installedAt"],
        before["skills"]["animate"]["installedAt"]
    );
}

#[test]
fn recording_refuses_a_lock_that_lost_the_entry() {
    let tree = TempTree::new();
    tree.write_skills_lock(json!({"other": entry("skills/other/SKILL.md")}));
    let path = tree.paths.skills_cli_lock();
    let before = std::fs::read(&path).unwrap();

    let result = record_skill_update(&path, "animate", NEW_HASH, "2026-10-24T10:00:00Z");

    assert!(matches!(result, Err(CoreError::SkillsLockChanged { .. })));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn recording_into_a_missing_lock_is_a_changed_lock() {
    let tree = TempTree::new();
    let path = tree.paths.skills_cli_lock();

    let result = record_skill_update(&path, "animate", NEW_HASH, "2026-10-24T10:00:00Z");

    assert!(matches!(result, Err(CoreError::SkillsLockChanged { .. })));
    assert!(!path.exists());
}

#[test]
fn recording_into_an_unparseable_lock_is_a_changed_lock() {
    let tree = TempTree::new();
    let path = tree.paths.skills_cli_lock();
    tree.write("{ nope", &path);

    let result = record_skill_update(&path, "animate", NEW_HASH, "2026-10-24T10:00:00Z");

    assert!(matches!(result, Err(CoreError::SkillsLockChanged { .. })));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ nope");
}

#[test]
fn a_lock_that_cannot_be_read_is_an_io_error() {
    let tree = TempTree::new();
    let path = tree.paths.skills_cli_lock();
    tree.make_directory(&path);

    let result = record_skill_update(&path, "animate", NEW_HASH, "2026-10-24T10:00:00Z");

    assert!(matches!(result, Err(CoreError::Io(_))));
}

#[test]
fn recording_refuses_a_lock_of_another_version() {
    let tree = TempTree::new();
    let path = tree.paths.skills_cli_lock();
    tree.write(
        &json!({"version": 4, "skills": {"animate": entry("skills/animate/SKILL.md")}}).to_string(),
        &path,
    );
    let before = std::fs::read(&path).unwrap();

    let result = record_skill_update(&path, "animate", NEW_HASH, "2026-10-24T10:00:00Z");

    assert!(matches!(result, Err(CoreError::SkillsLockChanged { .. })));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_skills_cli_skill_is_linked_from_the_claude_skills_folder() {
    let tree = TempTree::new();
    let link = tree.make_skills_cli_skill("animate");
    assert_eq!(link, tree.paths.skills_dir().join("animate"));
    assert!(link.join("SKILL.md").is_file());
    assert_eq!(
        std::fs::canonicalize(&link).unwrap(),
        std::fs::canonicalize(tree.paths.skills_cli_skills_dir().join("animate")).unwrap()
    );
}

#[test]
fn replacing_a_skill_folder_swaps_it_and_trashes_the_old_copy() {
    let tree = TempTree::new();
    let target = tree.root().join("skills").join("animate");
    tree.write("old", &target.join("SKILL.md"));
    let staged = tree.root().join("staging").join("animate");
    tree.write("new", &staged.join("SKILL.md"));
    let set_aside = tree.root().join(".replaced-animate");
    let trasher = FakeTrasher::new();

    replace_skill_folder(&staged, &target, &set_aside, &trasher).unwrap();

    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "new"
    );
    assert!(!staged.exists());
    assert_eq!(trasher.trashed(), vec![set_aside]);
}

#[test]
fn replacing_a_skill_folder_parks_the_old_copy_under_its_own_name() {
    let tree = TempTree::new();
    let target = tree.root().join("skills").join("animate");
    tree.write("old", &target.join("SKILL.md"));
    let staged = tree.root().join("staging").join("animate");
    tree.write("new", &staged.join("SKILL.md"));
    let set_aside = tree.root().join(".fettlr-replaced").join("animate");
    let trasher = FakeTrasher::new();

    replace_skill_folder(&staged, &target, &set_aside, &trasher).unwrap();

    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "new"
    );
    assert_eq!(
        std::fs::read_to_string(set_aside.join("SKILL.md")).unwrap(),
        "old"
    );
    assert_eq!(trasher.trashed(), vec![set_aside]);
}

#[test]
fn a_failed_swap_puts_the_old_folder_back() {
    let tree = TempTree::new();
    let target = tree.root().join("skills").join("animate");
    tree.write("old", &target.join("SKILL.md"));
    let staged = tree.root().join("staging").join("missing");
    let set_aside = tree.root().join(".replaced-animate");
    let trasher = FakeTrasher::new();

    assert!(replace_skill_folder(&staged, &target, &set_aside, &trasher).is_err());

    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "old"
    );
    assert!(!set_aside.exists());
    assert!(trasher.trashed().is_empty());
}
