use std::path::PathBuf;
use std::sync::Arc;

use fettlr_core::testing::{FakeClock, FakeTrasher, TempTree};
use fettlr_core::{CoreError, ParkKind, ParkedSkill, Scope, SkillStore};

const FIXED_NOW: &str = "2026-08-30T10:00:00Z";

fn store(tree: &TempTree, trasher: Arc<FakeTrasher>) -> SkillStore {
    SkillStore::for_kind(
        ParkKind::Subagent,
        &tree.paths,
        trasher,
        Arc::new(FakeClock::new(FIXED_NOW)),
    )
}

fn disabled_file(tree: &TempTree, folder: &str, file: &str) -> PathBuf {
    tree.paths.agents_disabled_dir().join(folder).join(file)
}

fn records_file(tree: &TempTree) -> PathBuf {
    tree.paths.agents_disabled_dir().join("parked.json")
}

#[test]
fn park_moves_the_file_and_records_it() {
    let tree = TempTree::new();
    let file = tree.make_subagent(
        "reviewer",
        &tree.user_agents_dir(),
        Some(&TempTree::subagent_markdown("reviewer", "Reviews.")),
    );
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let parked = store
        .park_path("reviewer", &Scope::User, &file, "reviewer.md")
        .expect("park succeeds");

    let destination = disabled_file(&tree, "user", "reviewer.md");
    assert!(!file.exists());
    assert!(destination.is_file());
    assert_eq!(parked.name, "reviewer");
    assert_eq!(parked.scope_key, "user");
    assert_eq!(parked.original_path, file.display().to_string());
    assert_eq!(parked.parked_path, destination.display().to_string());
    assert_eq!(parked.parked_at, FIXED_NOW);
    assert!(records_file(&tree).exists());
    assert!(
        !tree
            .paths
            .skills_disabled_dir()
            .join("parked.json")
            .exists()
    );
    assert_eq!(store.parked().expect("load"), vec![parked]);
}

#[test]
fn park_refuses_a_missing_file() {
    let tree = TempTree::new();
    let store = store(&tree, Arc::new(FakeTrasher::new()));
    let missing = tree.user_agents_dir().join("ghost.md");

    let error = store
        .park_path("ghost", &Scope::User, &missing, "ghost.md")
        .expect_err("missing file is refused");

    assert!(matches!(error, CoreError::MissingSubagent { .. }));
}

#[test]
fn park_refuses_when_the_destination_is_occupied() {
    let tree = TempTree::new();
    let file = tree.make_subagent("reviewer", &tree.user_agents_dir(), None);
    tree.write("older", &disabled_file(&tree, "user", "reviewer.md"));
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let error = store
        .park_path("reviewer", &Scope::User, &file, "reviewer.md")
        .expect_err("occupied destination is refused");

    assert!(matches!(error, CoreError::OccupiedDestination { .. }));
    assert!(file.exists());
}

#[test]
fn restore_moves_the_file_back_and_clears_the_record() {
    let tree = TempTree::new();
    let file = tree.make_subagent("reviewer", &tree.user_agents_dir(), None);
    let store = store(&tree, Arc::new(FakeTrasher::new()));
    let parked = store
        .park_path("reviewer", &Scope::User, &file, "reviewer.md")
        .expect("park");

    store.restore(&parked).expect("restore succeeds");

    assert!(file.is_file());
    assert!(!PathBuf::from(&parked.parked_path).exists());
    assert!(store.parked().expect("load").is_empty());
}

#[test]
fn restore_refuses_when_the_original_location_is_occupied() {
    let tree = TempTree::new();
    let file = tree.make_subagent("reviewer", &tree.user_agents_dir(), None);
    let store = store(&tree, Arc::new(FakeTrasher::new()));
    let parked = store
        .park_path("reviewer", &Scope::User, &file, "reviewer.md")
        .expect("park");
    tree.write("newer", &file);

    let error = store
        .restore(&parked)
        .expect_err("occupied original is refused");

    assert!(matches!(error, CoreError::OccupiedDestination { .. }));
    assert_eq!(store.parked().expect("load"), vec![parked]);
}

#[test]
fn delete_path_trashes_the_file() {
    let tree = TempTree::new();
    let file = tree.make_subagent("reviewer", &tree.user_agents_dir(), None);
    let trasher = Arc::new(FakeTrasher::new());
    let store = store(&tree, trasher.clone());

    store.delete_path(&file).expect("delete succeeds");

    assert_eq!(trasher.trashed(), vec![file]);
}

#[test]
fn delete_parked_clears_the_record_then_trashes_the_file() {
    let tree = TempTree::new();
    let file = tree.make_subagent("reviewer", &tree.user_agents_dir(), None);
    let trasher = Arc::new(FakeTrasher::new());
    let store = store(&tree, trasher.clone());
    let parked = store
        .park_path("reviewer", &Scope::User, &file, "reviewer.md")
        .expect("park");

    store
        .delete_parked(&parked)
        .expect("delete parked succeeds");

    assert!(store.parked().expect("load").is_empty());
    assert_eq!(trasher.trashed(), vec![PathBuf::from(&parked.parked_path)]);
}

#[test]
fn two_subagents_sharing_a_name_in_different_folders_both_stay_recorded() {
    let tree = TempTree::new();
    let first = tree.make_subagent("sec", &tree.user_agents_dir().join("review"), None);
    let second = tree.make_subagent("security", &tree.user_agents_dir().join("audit"), None);
    let store = store(&tree, Arc::new(FakeTrasher::new()));

    let parked_first = store
        .park_path("security", &Scope::User, &first, "review/sec.md")
        .expect("park first");
    let parked_second = store
        .park_path("security", &Scope::User, &second, "audit/security.md")
        .expect("park second");

    let records = store.parked().expect("load");
    assert_eq!(records, vec![parked_first.clone(), parked_second.clone()]);
    assert_eq!(
        PathBuf::from(&parked_first.parked_path),
        disabled_file(&tree, "user", "review/sec.md")
    );
    assert_eq!(
        PathBuf::from(&parked_second.parked_path),
        disabled_file(&tree, "user", "audit/security.md")
    );
    assert!(disabled_file(&tree, "user", "review/sec.md").is_file());
    assert!(disabled_file(&tree, "user", "audit/security.md").is_file());
    store.restore(&parked_first).expect("restore first");
    assert!(first.is_file());
    assert_eq!(store.parked().expect("load"), vec![parked_second]);
}

#[test]
fn project_scope_gets_its_own_folder() {
    let tree = TempTree::new();
    let project = tree.make_project("example-project");
    let file = tree.make_subagent("deployer", &tree.project_agents_dir(&project), None);
    let store = store(&tree, Arc::new(FakeTrasher::new()));
    let scope = Scope::Project {
        path: project.display().to_string(),
    };

    let parked: ParkedSkill = store
        .park_path("deployer", &scope, &file, "deployer.md")
        .expect("park");

    assert!(parked.scope_key.starts_with("project-example-project-"));
    assert_eq!(
        PathBuf::from(&parked.parked_path),
        disabled_file(&tree, &parked.scope_key, "deployer.md")
    );
}
