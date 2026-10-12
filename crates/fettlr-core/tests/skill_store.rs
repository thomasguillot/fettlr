use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fettlr_core::testing::{FakeClock, FakeTrasher, TempTree};
use fettlr_core::{CoreError, ParkedSkill, Provenance, Scope, Skill, SkillStore};

const FIXED_NOW: &str = "2026-08-23T10:00:00Z";

fn skill(name: &str, scope: Scope, path: PathBuf) -> Skill {
    Skill {
        id: format!("{}/{name}", scope.key()),
        name: name.to_string(),
        title: None,
        summary: None,
        scope,
        provenance: Provenance::Local,
        path,
    }
}

fn store(tree: &TempTree, trasher: Arc<FakeTrasher>) -> SkillStore {
    SkillStore::with_clock(&tree.paths, trasher, Arc::new(FakeClock::new(FIXED_NOW)))
}

fn disabled_dir(tree: &TempTree, folder: &str, name: &str) -> PathBuf {
    tree.paths.skills_disabled_dir().join(folder).join(name)
}

fn parked_json(tree: &TempTree) -> PathBuf {
    tree.paths.skills_disabled_dir().join("parked.json")
}

fn scope_folder_name(parked: &ParkedSkill) -> String {
    Path::new(&parked.parked_path)
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .expect("parked path has a parent folder")
        .to_string()
}

struct RestorePermissions {
    path: PathBuf,
}

impl Drop for RestorePermissions {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn park_moves_the_folder_and_records_it() {
    let tree = TempTree::new();
    let folder = tree.make_skill(
        "alpha",
        &tree.user_skills_dir(),
        Some(&TempTree::skill_markdown("alpha", "The first skill.")),
        false,
    );
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let parked = store
        .park(&skill("alpha", Scope::User, folder.clone()))
        .expect("park succeeds");

    let destination = disabled_dir(&tree, "user", "alpha");
    assert!(!folder.exists());
    assert!(destination.join("SKILL.md").exists());
    assert_eq!(parked.name, "alpha");
    assert_eq!(parked.scope_key, "user");
    assert_eq!(parked.original_path, folder.display().to_string());
    assert_eq!(parked.parked_path, destination.display().to_string());
    assert_eq!(parked.parked_at, FIXED_NOW);

    assert!(parked_json(&tree).exists());
    let records = store.parked().expect("load succeeds");
    assert_eq!(records, vec![parked]);
}

#[test]
fn parked_skills_is_empty_before_anything_is_parked() {
    let tree = TempTree::new();
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let records = store.parked().expect("load succeeds");

    assert!(records.is_empty());
}

#[test]
fn restore_moves_the_folder_back_and_drops_the_record() {
    let tree = TempTree::new();
    let folder = tree.make_skill(
        "alpha",
        &tree.user_skills_dir(),
        Some(&TempTree::skill_markdown("alpha", "The first skill.")),
        false,
    );
    let store = store(&tree, Arc::new(FakeTrasher::new()));
    let parked = store
        .park(&skill("alpha", Scope::User, folder.clone()))
        .expect("park succeeds");

    store.restore(&parked).expect("restore succeeds");

    assert!(folder.join("SKILL.md").exists());
    assert!(!Path::new(&parked.parked_path).exists());
    let records = store.parked().expect("load succeeds");
    assert!(records.is_empty());
}

#[test]
fn restore_onto_an_occupied_path_throws_and_leaves_both_folders_intact() {
    let tree = TempTree::new();
    let folder = tree.make_skill(
        "alpha",
        &tree.user_skills_dir(),
        Some(&TempTree::skill_markdown("alpha", "Parked copy.")),
        false,
    );
    let store = store(&tree, Arc::new(FakeTrasher::new()));
    let parked = store
        .park(&skill("alpha", Scope::User, folder.clone()))
        .expect("park succeeds");
    tree.make_skill(
        "alpha",
        &tree.user_skills_dir(),
        Some(&TempTree::skill_markdown("alpha", "Replacement.")),
        false,
    );

    let error = store.restore(&parked).expect_err("restore fails");

    match error {
        CoreError::OccupiedDestination { path } => assert_eq!(path, folder.display().to_string()),
        other => panic!("expected OccupiedDestination, got {other:?}"),
    }
    let live = std::fs::read_to_string(folder.join("SKILL.md")).expect("read live skill");
    assert!(live.contains("Replacement."));
    let still_parked = std::fs::read_to_string(Path::new(&parked.parked_path).join("SKILL.md"))
        .expect("read parked skill");
    assert!(still_parked.contains("Parked copy."));
    let records = store.parked().expect("load succeeds");
    assert_eq!(records, vec![parked]);
}

#[test]
fn parking_the_same_name_in_two_scopes_does_not_collide() {
    let tree = TempTree::new();
    let project = tree.make_project("example-project");
    let user_folder = tree.make_skill(
        "alpha",
        &tree.user_skills_dir(),
        Some(&TempTree::skill_markdown("alpha", "User copy.")),
        false,
    );
    let project_folder = tree.make_skill(
        "alpha",
        &tree.project_skills_dir(&project),
        Some(&TempTree::skill_markdown("alpha", "Project copy.")),
        false,
    );
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let parked_user = store
        .park(&skill("alpha", Scope::User, user_folder))
        .expect("park user succeeds");
    let parked_project = store
        .park(&skill(
            "alpha",
            Scope::Project {
                path: project.display().to_string(),
            },
            project_folder,
        ))
        .expect("park project succeeds");

    assert_eq!(
        parked_user.parked_path,
        disabled_dir(&tree, "user", "alpha").display().to_string()
    );
    let project_folder_name = scope_folder_name(&parked_project);
    assert!(project_folder_name.starts_with("project-example-project-"));
    assert!(!project_folder_name.contains(':'));
    let digest = &project_folder_name["project-example-project-".len()..];
    assert_eq!(digest.len(), 8);
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));

    let user_copy =
        std::fs::read_to_string(Path::new(&parked_user.parked_path).join("SKILL.md")).unwrap();
    let project_copy =
        std::fs::read_to_string(Path::new(&parked_project.parked_path).join("SKILL.md")).unwrap();
    assert!(user_copy.contains("User copy."));
    assert!(project_copy.contains("Project copy."));

    let records = store.parked().expect("load succeeds");
    let mut pairs: Vec<(String, String)> = records
        .iter()
        .map(|record| (record.scope_key.clone(), record.name.clone()))
        .collect();
    pairs.sort();
    let mut expected = vec![
        ("user".to_string(), "alpha".to_string()),
        (project_folder_name, "alpha".to_string()),
    ];
    expected.sort();
    assert_eq!(pairs, expected);
}

#[test]
fn project_paths_that_sanitize_alike_get_distinct_folders() {
    let tree = TempTree::new();
    let dashed = tree.make_project("alpha-one");
    let scored = tree.make_project("alpha_one");
    let dashed_folder = tree.make_skill(
        "alpha",
        &tree.project_skills_dir(&dashed),
        Some(&TempTree::skill_markdown("alpha", "Dashed project.")),
        false,
    );
    let scored_folder = tree.make_skill(
        "alpha",
        &tree.project_skills_dir(&scored),
        Some(&TempTree::skill_markdown("alpha", "Scored project.")),
        false,
    );
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let parked_dashed = store
        .park(&skill(
            "alpha",
            Scope::Project {
                path: dashed.display().to_string(),
            },
            dashed_folder,
        ))
        .expect("park dashed succeeds");
    let parked_scored = store
        .park(&skill(
            "alpha",
            Scope::Project {
                path: scored.display().to_string(),
            },
            scored_folder,
        ))
        .expect("park scored succeeds");

    assert!(scope_folder_name(&parked_dashed).starts_with("project-alpha-one-"));
    assert!(scope_folder_name(&parked_scored).starts_with("project-alpha-one-"));
    assert_ne!(
        scope_folder_name(&parked_dashed),
        scope_folder_name(&parked_scored)
    );
    let dashed_copy =
        std::fs::read_to_string(Path::new(&parked_dashed.parked_path).join("SKILL.md")).unwrap();
    let scored_copy =
        std::fs::read_to_string(Path::new(&parked_scored.parked_path).join("SKILL.md")).unwrap();
    assert!(dashed_copy.contains("Dashed project."));
    assert!(scored_copy.contains("Scored project."));
}

#[test]
fn park_onto_an_occupied_path_throws_and_leaves_the_skill_in_place() {
    let tree = TempTree::new();
    let folder = tree.make_skill(
        "alpha",
        &tree.user_skills_dir(),
        Some(&TempTree::skill_markdown("alpha", "Live copy.")),
        false,
    );
    let occupied = disabled_dir(&tree, "user", "alpha");
    tree.write("leftover", &occupied.join("SKILL.md"));
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let error = store
        .park(&skill("alpha", Scope::User, folder.clone()))
        .expect_err("park fails");

    match error {
        CoreError::OccupiedDestination { path } => {
            assert_eq!(path, occupied.display().to_string())
        }
        other => panic!("expected OccupiedDestination, got {other:?}"),
    }
    let live = std::fs::read_to_string(folder.join("SKILL.md")).unwrap();
    assert!(live.contains("Live copy."));
    let leftover = std::fs::read_to_string(occupied.join("SKILL.md")).unwrap();
    assert_eq!(leftover, "leftover");
}

#[test]
fn parking_a_missing_folder_throws() {
    let tree = TempTree::new();
    let missing = tree.user_skills_dir().join("ghost");
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let error = store
        .park(&skill("ghost", Scope::User, missing.clone()))
        .expect_err("park fails");

    match error {
        CoreError::MissingSkill { path } => assert_eq!(path, missing.display().to_string()),
        other => panic!("expected MissingSkill, got {other:?}"),
    }
}

#[test]
fn a_failed_record_write_moves_the_skill_back_and_tidies_up() {
    let tree = TempTree::new();
    let project = tree.make_project("example-project");
    let first = tree.make_skill("alpha", &tree.user_skills_dir(), None, false);
    let second = tree.make_skill(
        "bravo",
        &tree.project_skills_dir(&project),
        Some(&TempTree::skill_markdown("bravo", "Second skill.")),
        false,
    );
    let store = store(&tree, Arc::new(FakeTrasher::new()));
    let parked_first = store
        .park(&skill("alpha", Scope::User, first))
        .expect("park first succeeds");

    let records_dir = tree.paths.skills_disabled_dir();
    std::fs::set_permissions(&records_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let _restore = RestorePermissions {
        path: records_dir.clone(),
    };

    let error = store.park(&skill(
        "bravo",
        Scope::Project {
            path: project.display().to_string(),
        },
        second.clone(),
    ));
    assert!(error.is_err());

    assert!(second.join("SKILL.md").exists());
    let mut scope_folders: Vec<String> = std::fs::read_dir(tree.paths.skills_disabled_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != "parked.json")
        .collect();
    scope_folders.sort();
    assert_eq!(scope_folders, vec!["user".to_string()]);
    assert!(Path::new(&parked_first.parked_path).exists());
}

#[test]
fn delete_trashes_the_skill_folder_without_removing_it() {
    let tree = TempTree::new();
    let folder = tree.make_skill("alpha", &tree.user_skills_dir(), None, false);
    let trasher = Arc::new(FakeTrasher::new());
    let store = store(&tree, trasher.clone());

    store
        .delete(&skill("alpha", Scope::User, folder.clone()))
        .expect("delete succeeds");

    assert_eq!(trasher.trashed(), vec![folder.clone()]);
    assert!(folder.exists());
}

#[test]
fn delete_parked_trashes_the_parked_folder_and_drops_the_record() {
    let tree = TempTree::new();
    let folder = tree.make_skill("alpha", &tree.user_skills_dir(), None, false);
    let trasher = Arc::new(FakeTrasher::new());
    let store = store(&tree, trasher.clone());
    let parked = store
        .park(&skill("alpha", Scope::User, folder))
        .expect("park succeeds");

    store
        .delete_parked(&parked)
        .expect("delete_parked succeeds");

    assert_eq!(trasher.trashed(), vec![PathBuf::from(&parked.parked_path)]);
    let records = store.parked().expect("load succeeds");
    assert!(records.is_empty());
}

#[test]
fn delete_parked_clears_a_record_whose_folder_is_gone() {
    let tree = TempTree::new();
    let folder = tree.make_skill("alpha", &tree.user_skills_dir(), None, false);
    let trasher = Arc::new(FakeTrasher::new());
    let store = store(&tree, trasher.clone());
    let parked = store
        .park(&skill("alpha", Scope::User, folder))
        .expect("park succeeds");
    std::fs::remove_dir_all(&parked.parked_path).unwrap();

    store
        .delete_parked(&parked)
        .expect("delete_parked succeeds");

    assert!(trasher.trashed().is_empty());
    let records = store.parked().expect("load succeeds");
    assert!(records.is_empty());
}

#[test]
fn a_corrupt_store_is_copied_aside_rebuilt_and_surfaced_in_the_error() {
    let tree = TempTree::new();
    let garbage = "{ this is not the store you are looking for";
    tree.write(garbage, &parked_json(&tree));
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let error = store.parked().expect_err("load fails");
    let message = error.to_string();

    let (reason, backup) = match error {
        CoreError::CorruptStore {
            path,
            reason,
            backup,
        } => {
            assert_eq!(path, parked_json(&tree).display().to_string());
            (reason, backup)
        }
        other => panic!("expected CorruptStore, got {other:?}"),
    };
    assert!(!reason.is_empty());
    let backup = backup.expect("backup path present");
    let backup_name = Path::new(&backup)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap()
        .to_string();
    assert!(
        backup_name.starts_with("parked.json.corrupt-"),
        "backup name was {backup_name}"
    );
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), garbage);
    assert!(message.contains(&backup), "display message was {message}");

    let rebuilt = store.parked().expect("load succeeds after rebuild");
    assert!(rebuilt.is_empty());
}

#[test]
fn an_unreadable_store_that_cannot_be_copied_aside_is_left_as_it_is() {
    let tree = TempTree::new();
    let garbage = "{ this is not the store you are looking for";
    tree.write(garbage, &parked_json(&tree));
    let records_dir = tree.paths.skills_disabled_dir();
    std::fs::set_permissions(&records_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let _restore = RestorePermissions {
        path: records_dir.clone(),
    };
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let error = store.parked().expect_err("load fails");
    let message = error.to_string();

    let backup = match error {
        CoreError::CorruptStore { reason, backup, .. } => {
            assert!(!reason.is_empty());
            backup
        }
        other => panic!("expected CorruptStore, got {other:?}"),
    };
    assert!(backup.is_none());
    assert!(
        !message.contains("kept a copy"),
        "display message was {message}"
    );
    assert_eq!(
        std::fs::read_to_string(parked_json(&tree)).unwrap(),
        garbage
    );
    let leftovers: Vec<String> = std::fs::read_dir(tree.paths.skills_disabled_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, vec!["parked.json".to_string()]);
}

#[test]
fn park_still_works_after_a_corrupt_store_is_rebuilt() {
    let tree = TempTree::new();
    tree.write("[[[", &parked_json(&tree));
    let folder = tree.make_skill("alpha", &tree.user_skills_dir(), None, false);
    let store = store(&tree, Arc::new(FakeTrasher::new()));
    let alpha = skill("alpha", Scope::User, folder.clone());

    let first_attempt = store.park(&alpha);
    assert!(first_attempt.is_err());
    assert!(folder.exists());

    let parked = store.park(&alpha).expect("second park succeeds");

    assert_eq!(
        parked.parked_path,
        disabled_dir(&tree, "user", "alpha").display().to_string()
    );
    let records = store.parked().expect("load succeeds");
    assert_eq!(records, vec![parked]);
}

#[test]
fn two_parks_at_once_keep_both_records() {
    let tree = TempTree::new();
    let store = Arc::new(store(&tree, Arc::new(FakeTrasher::new())));

    let skills: Vec<Skill> = (0..8)
        .map(|index| {
            let name = format!("skill-{index}");
            let path = tree.make_skill(
                &name,
                &tree.user_skills_dir(),
                Some(&TempTree::skill_markdown(&name, "One of many.")),
                false,
            );
            skill(&name, Scope::User, path)
        })
        .collect();

    let threads: Vec<_> = skills
        .into_iter()
        .map(|skill| {
            let store = Arc::clone(&store);
            std::thread::spawn(move || store.park(&skill).expect("parks"))
        })
        .collect();
    for thread in threads {
        thread.join().expect("the park finishes");
    }

    let records = store.parked().expect("read the records");
    assert_eq!(
        records.len(),
        8,
        "an interleaved park dropped a record, stranding a skill folder with no way back"
    );
}
