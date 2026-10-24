use std::fs;
use std::io::Read;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use fettlr_core::{CoreError, write_skill_zip};
use tempfile::TempDir;

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("has parent")).expect("create dirs");
    fs::write(path, content).expect("write");
}

fn entry_names(zip_path: &Path) -> Vec<String> {
    let archive = zip::ZipArchive::new(fs::File::open(zip_path).expect("open")).expect("zip");
    archive
        .file_names()
        .map(|name| name.expect("name").into_owned())
        .collect()
}

fn demo_skill(dir: &TempDir) -> std::path::PathBuf {
    let source = dir.path().join("demo");
    write(&source.join("SKILL.md"), "---\nname: demo\n---\nbody\n");
    write(&source.join("reference/a.md"), "a");
    write(&source.join("scripts/run.sh"), "#!/bin/sh\n");
    fs::set_permissions(
        source.join("scripts/run.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("chmod");
    source
}

#[test]
fn the_zip_holds_the_skill_folder_at_its_root() {
    let dir = TempDir::new().expect("tempdir");
    let source = demo_skill(&dir);
    let destination = dir.path().join("out/demo.zip");

    write_skill_zip(&source, "demo", &destination).expect("zip");

    assert_eq!(
        entry_names(&destination),
        vec![
            "demo/",
            "demo/SKILL.md",
            "demo/reference/a.md",
            "demo/scripts/run.sh"
        ]
    );
    let mut archive =
        zip::ZipArchive::new(fs::File::open(&destination).expect("open")).expect("zip");
    let mut content = String::new();
    archive
        .by_name("demo/SKILL.md")
        .expect("entry")
        .read_to_string(&mut content)
        .expect("read");
    assert_eq!(content, "---\nname: demo\n---\nbody\n");
    assert_eq!(
        archive
            .by_name("demo/scripts/run.sh")
            .expect("entry")
            .unix_mode(),
        Some(0o100755)
    );
}

#[test]
fn dotfiles_and_symlinks_stay_out_of_the_zip() {
    let dir = TempDir::new().expect("tempdir");
    let source = demo_skill(&dir);
    write(&source.join(".env"), "SECRET=1");
    write(&source.join(".git/config"), "x");
    write(&source.join(".DS_Store"), "x");
    let outside = dir.path().join("outside.md");
    write(&outside, "private");
    symlink(&outside, source.join("secrets.md")).expect("symlink");
    let destination = dir.path().join("demo.zip");

    write_skill_zip(&source, "demo", &destination).expect("zip");

    let names = entry_names(&destination);
    for hidden in [".env", ".git", ".DS_Store", "secrets.md"] {
        assert!(
            !names.iter().any(|name| name.contains(hidden)),
            "{hidden} leaked into {names:?}"
        );
    }
}

#[test]
fn a_symlinked_directory_and_a_nested_dotfile_stay_out_of_the_zip() {
    let dir = TempDir::new().expect("tempdir");
    let source = demo_skill(&dir);
    let outside = dir.path().join("outside");
    write(&outside.join("leaked.md"), "private");
    symlink(&outside, source.join("docs")).expect("symlink");
    write(&source.join("reference/.secret"), "SECRET=1");
    let destination = dir.path().join("demo.zip");

    write_skill_zip(&source, "demo", &destination).expect("zip");

    assert_eq!(
        entry_names(&destination),
        vec![
            "demo/",
            "demo/SKILL.md",
            "demo/reference/a.md",
            "demo/scripts/run.sh"
        ]
    );
}

#[test]
fn a_folder_without_skill_md_is_refused() {
    let dir = TempDir::new().expect("tempdir");
    let source = dir.path().join("demo");
    write(&source.join("reference/a.md"), "a");
    let destination = dir.path().join("demo.zip");

    let result = write_skill_zip(&source, "demo", &destination);

    assert!(matches!(result, Err(CoreError::UnpackageableSkill { .. })));
    assert!(!destination.exists());
}

#[test]
fn a_second_package_replaces_the_first() {
    let dir = TempDir::new().expect("tempdir");
    let source = demo_skill(&dir);
    let destination = dir.path().join("out/demo.zip");

    write_skill_zip(&source, "demo", &destination).expect("first");
    write(&source.join("SKILL.md"), "second");
    write_skill_zip(&source, "demo", &destination).expect("second");

    assert_eq!(
        fs::read_dir(destination.parent().expect("parent"))
            .expect("dir")
            .count(),
        1
    );
    let mut archive =
        zip::ZipArchive::new(fs::File::open(&destination).expect("open")).expect("zip");
    let mut content = String::new();
    archive
        .by_name("demo/SKILL.md")
        .expect("entry")
        .read_to_string(&mut content)
        .expect("read");
    assert_eq!(content, "second");
}

#[test]
fn a_folder_with_too_many_files_is_refused() {
    let dir = TempDir::new().expect("tempdir");
    let source = demo_skill(&dir);
    for index in 0..5001 {
        write(&source.join(format!("bulk/f{index}.txt")), "x");
    }
    let destination = dir.path().join("demo.zip");

    let result = write_skill_zip(&source, "demo", &destination);

    assert!(matches!(result, Err(CoreError::UnpackageableSkill { .. })));
    assert!(!destination.exists());
}

#[test]
fn a_folder_nested_too_deep_is_refused() {
    let dir = TempDir::new().expect("tempdir");
    let source = demo_skill(&dir);
    let mut deep = source.clone();
    for _ in 0..17 {
        deep = deep.join("d");
    }
    write(&deep.join("f.txt"), "x");
    let destination = dir.path().join("demo.zip");

    let result = write_skill_zip(&source, "demo", &destination);

    assert!(matches!(result, Err(CoreError::UnpackageableSkill { .. })));
    assert!(!destination.exists());
}
