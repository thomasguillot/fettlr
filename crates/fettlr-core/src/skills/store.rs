use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::civil_time;
use crate::cli_records::CoreError;
use crate::json_store::{load_json_store, save_json_store};
use crate::models::Scope;
use crate::paths::Paths;
use crate::trashing::Trashing;

use super::Skill;

pub trait Clock: Send + Sync {
    fn now_rfc3339(&self) -> String;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        civil_time::format_rfc3339(civil_time::now_unix_seconds())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParkedSkill {
    pub name: String,
    pub scope_key: String,
    pub original_path: String,
    pub parked_path: String,
    pub parked_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParkKind {
    Skill,
    Subagent,
}

impl ParkKind {
    fn present(self, path: &Path) -> bool {
        match self {
            ParkKind::Skill => path.is_dir(),
            ParkKind::Subagent => path.is_file(),
        }
    }

    fn missing(self, path: &Path) -> CoreError {
        let path = path.display().to_string();
        match self {
            ParkKind::Skill => CoreError::MissingSkill { path },
            ParkKind::Subagent => CoreError::MissingSubagent { path },
        }
    }
}

pub type ParkedSubagent = ParkedSkill;

pub struct SkillStore {
    kind: ParkKind,
    paths: Paths,
    trasher: Arc<dyn Trashing>,
    clock: Arc<dyn Clock>,
    /// Serialises every read-modify-write of the records file, which two parks would otherwise
    /// interleave and lose a record between: the item moves under the disabled folder either
    /// way, so a dropped record leaves it in neither list and with no route back.
    records: Mutex<()>,
}

impl SkillStore {
    pub fn new(paths: &Paths, trasher: Arc<dyn Trashing>) -> Self {
        Self::with_clock(paths, trasher, Arc::new(SystemClock))
    }

    pub fn with_clock(paths: &Paths, trasher: Arc<dyn Trashing>, clock: Arc<dyn Clock>) -> Self {
        Self::for_kind(ParkKind::Skill, paths, trasher, clock)
    }

    pub fn for_kind(
        kind: ParkKind,
        paths: &Paths,
        trasher: Arc<dyn Trashing>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            kind,
            paths: paths.clone(),
            trasher,
            clock,
            records: Mutex::new(()),
        }
    }

    fn lock_records(&self) -> MutexGuard<'_, ()> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn read_records(&self) -> Result<Vec<ParkedSkill>, CoreError> {
        load_json_store(&self.parked_json_path())
    }

    pub fn parked(&self) -> Result<Vec<ParkedSkill>, CoreError> {
        let _records = self.lock_records();
        self.read_records()
    }

    pub fn park(&self, skill: &Skill) -> Result<ParkedSkill, CoreError> {
        self.park_path(&skill.name, &skill.scope, &skill.path, &skill.name)
    }

    pub fn park_path(
        &self,
        name: &str,
        scope: &Scope,
        path: &Path,
        leaf: &str,
    ) -> Result<ParkedSkill, CoreError> {
        let _records = self.lock_records();
        let mut records = self.read_records()?;
        if !self.kind.present(path) {
            return Err(self.kind.missing(path));
        }

        let scope_key = scope_folder_name(scope);
        let folder = self.disabled_dir().join(&scope_key);
        let destination = folder.join(leaf);
        if destination.exists() {
            return Err(CoreError::OccupiedDestination {
                path: destination.display().to_string(),
            });
        }

        let record = ParkedSkill {
            name: name.to_string(),
            scope_key,
            original_path: path.display().to_string(),
            parked_path: destination.display().to_string(),
            parked_at: self.clock.now_rfc3339(),
        };
        replace_record(&mut records, &record);

        let parent = destination.parent().unwrap_or(&folder).to_path_buf();
        std::fs::create_dir_all(&parent)?;
        if let Err(error) = self.move_and_record(path, &destination, &records) {
            remove_empty_folders(&parent, &folder);
            return Err(error);
        }
        Ok(record)
    }

    pub fn restore(&self, parked: &ParkedSkill) -> Result<(), CoreError> {
        let _records = self.lock_records();
        let mut records = self.read_records()?;
        let source = PathBuf::from(&parked.parked_path);
        let destination = PathBuf::from(&parked.original_path);
        if !self.kind.present(&source) {
            return Err(self.kind.missing(&source));
        }
        if destination.exists() {
            return Err(CoreError::OccupiedDestination {
                path: destination.display().to_string(),
            });
        }

        records.retain(|record| !matches_parked(record, parked));
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.move_and_record(&source, &destination, &records)
    }

    pub fn delete(&self, skill: &Skill) -> Result<(), CoreError> {
        self.delete_path(&skill.path)
    }

    pub fn delete_path(&self, path: &Path) -> Result<(), CoreError> {
        self.trasher.trash(path)
    }

    /// The record goes first: an orphaned folder is still recoverable, a record that cannot be
    /// cleared is not.
    pub fn delete_parked(&self, parked: &ParkedSkill) -> Result<(), CoreError> {
        {
            let _records = self.lock_records();
            let mut records = self.read_records()?;
            records.retain(|record| !matches_parked(record, parked));
            save_json_store(&self.parked_json_path(), &records)?;
        }

        let folder = PathBuf::from(&parked.parked_path);
        if !folder.exists() {
            return Ok(());
        }
        self.trasher.trash(&folder)
    }

    fn parked_json_path(&self) -> PathBuf {
        self.disabled_dir().join("parked.json")
    }

    fn disabled_dir(&self) -> PathBuf {
        match self.kind {
            ParkKind::Skill => self.paths.skills_disabled_dir(),
            ParkKind::Subagent => self.paths.agents_disabled_dir(),
        }
    }

    fn move_and_record(
        &self,
        source: &Path,
        destination: &Path,
        records: &[ParkedSkill],
    ) -> Result<(), CoreError> {
        let moved = move_path(source, destination)?;
        if let Err(save_error) = save_json_store(&self.parked_json_path(), &records.to_vec()) {
            if move_path(destination, source).is_err() {
                return Err(CoreError::StrandedSkill {
                    path: destination.display().to_string(),
                    reason: save_error.to_string(),
                });
            }
            return Err(save_error);
        }
        match moved {
            Moved::Complete => Ok(()),
            Moved::SourceLeftBehind(error) => Err(CoreError::SourceNotRemoved {
                source_path: source.display().to_string(),
                destination: destination.display().to_string(),
                reason: error.to_string(),
            }),
        }
    }
}

enum Moved {
    Complete,
    SourceLeftBehind(std::io::Error),
}

/// `rename(2)` fails with `EXDEV` when source and destination sit on different filesystems (an
/// external drive, a network mount); Foundation's `FileManager.moveItem` falls back to a copy in
/// that case, so this does too.
fn move_path(source: &Path, destination: &Path) -> Result<Moved, CoreError> {
    match std::fs::rename(source, destination) {
        Ok(()) => Ok(Moved::Complete),
        Err(error) if is_cross_device(&error) => copy_and_remove(source, destination),
        Err(error) => Err(error.into()),
    }
}

fn is_cross_device(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::CrossesDevices || raw_exdev(error)
}

#[cfg(unix)]
fn raw_exdev(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(libc::EXDEV)
}

#[cfg(not(unix))]
fn raw_exdev(_error: &std::io::Error) -> bool {
    false
}

fn copy_and_remove(source: &Path, destination: &Path) -> Result<Moved, CoreError> {
    if source.is_file() {
        if let Err(error) = std::fs::copy(source, destination) {
            let _ = std::fs::remove_file(destination);
            return Err(error.into());
        }
        return Ok(removed(std::fs::remove_file(source)));
    }
    if let Err(error) = copy_dir_recursive(source, destination) {
        let _ = std::fs::remove_dir_all(destination);
        return Err(error.into());
    }
    Ok(removed(std::fs::remove_dir_all(source)))
}

fn removed(result: std::io::Result<()>) -> Moved {
    match result {
        Ok(()) => Moved::Complete,
        Err(error) => Moved::SourceLeftBehind(error),
    }
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let entry_destination = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &entry_destination)?;
        } else if file_type.is_symlink() {
            copy_symlink(&entry.path(), &entry_destination)?;
        } else {
            std::fs::copy(entry.path(), &entry_destination)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_symlink(source: &Path, destination: &Path) -> std::io::Result<()> {
    let target = std::fs::read_link(source)?;
    std::os::unix::fs::symlink(target, destination)
}

/// Windows support is a later milestone; until then a symlinked skill folder can't be copied
/// across a cross-device move on that platform, so this fails clearly instead of silently
/// dropping the entry.
#[cfg(not(unix))]
fn copy_symlink(source: &Path, _destination: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!(
            "cannot copy the symlink at {} on this platform",
            source.display()
        ),
    ))
}

fn matches_parked(a: &ParkedSkill, b: &ParkedSkill) -> bool {
    a.original_path == b.original_path
}

fn replace_record(records: &mut Vec<ParkedSkill>, record: &ParkedSkill) {
    records.retain(|existing| !matches_parked(existing, record));
    records.push(record.clone());
}

fn remove_empty_folders(from: &Path, root: &Path) {
    let mut current = from;
    loop {
        if !remove_if_empty(current) || current == root {
            return;
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return,
        }
    }
}

fn remove_if_empty(directory: &Path) -> bool {
    let empty = std::fs::read_dir(directory)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false);
    empty && std::fs::remove_dir(directory).is_ok()
}

/// Injective and Finder-safe: a path-derived slug can collide across projects that sanitize
/// alike, so a hash of the full path is appended.
fn scope_folder_name(scope: &Scope) -> String {
    let Some(path) = scope.project_path() else {
        return "user".to_string();
    };
    format!(
        "project-{}-{}",
        sanitized_basename(path),
        sha256_hex_prefix(path)
    )
}

fn sanitized_basename(path: &str) -> String {
    let basename = Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    basename
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn sha256_hex_prefix(path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    hasher.finalize()[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn copy_and_remove_copies_the_tree_and_removes_the_source() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let source = dir.path().join("source");
        std::fs::create_dir_all(source.join("nested")).expect("create nested dir");
        std::fs::write(source.join("SKILL.md"), "root file").expect("write root file");
        std::fs::write(source.join("nested/note.txt"), "nested file").expect("write nested file");
        let link_target = source.join("SKILL.md");
        std::os::unix::fs::symlink(&link_target, source.join("link.md")).expect("create symlink");
        let destination = dir.path().join("destination");

        copy_and_remove(&source, &destination).expect("copy and remove succeeds");

        assert!(!source.exists());
        assert_eq!(
            std::fs::read_to_string(destination.join("SKILL.md")).unwrap(),
            "root file"
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("nested/note.txt")).unwrap(),
            "nested file"
        );
        assert_eq!(
            std::fs::read_link(destination.join("link.md")).unwrap(),
            link_target
        );
    }

    #[test]
    fn copy_and_remove_moves_a_single_file() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let source = dir.path().join("agent.md");
        std::fs::write(&source, "---\nname: a\n---\n").expect("write");
        let destination = dir.path().join("moved.md");

        copy_and_remove(&source, &destination).expect("copy and remove succeeds");

        assert!(!source.exists());
        assert_eq!(
            std::fs::read_to_string(destination).unwrap(),
            "---\nname: a\n---\n"
        );
    }

    #[test]
    fn copy_and_remove_cleans_up_the_partial_destination_on_copy_failure() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let source = dir.path().join("source");
        std::fs::create_dir_all(&source).expect("create source dir");
        std::fs::write(source.join("ok.txt"), "fine").expect("write ok file");
        let blocked = source.join("blocked.txt");
        std::fs::write(&blocked, "unreadable").expect("write blocked file");
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o000))
            .expect("remove read permission");
        let destination = dir.path().join("destination");

        let result = copy_and_remove(&source, &destination);

        let _ = std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o644));
        assert!(result.is_err());
        assert!(!destination.exists());
        assert!(source.exists());
        assert!(blocked.exists());
    }

    #[test]
    fn is_cross_device_recognizes_the_raw_exdev_errno() {
        let error = std::io::Error::from_raw_os_error(libc::EXDEV);
        assert!(is_cross_device(&error));
    }

    #[test]
    fn is_cross_device_rejects_an_unrelated_error() {
        let error = std::io::Error::from_raw_os_error(libc::ENOENT);
        assert!(!is_cross_device(&error));
    }

    #[test]
    fn move_path_moves_within_the_same_device_via_rename() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let source = dir.path().join("source");
        std::fs::create_dir_all(&source).expect("create source dir");
        std::fs::write(source.join("SKILL.md"), "same device").expect("write file");
        let destination = dir.path().join("destination");

        move_path(&source, &destination).expect("move succeeds");

        assert!(!source.exists());
        assert_eq!(
            std::fs::read_to_string(destination.join("SKILL.md")).unwrap(),
            "same device"
        );
    }

    #[test]
    fn copy_and_remove_reports_a_source_it_could_not_remove_and_keeps_the_copy() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let source = dir.path().join("source");
        std::fs::create_dir_all(source.join("locked")).expect("create locked dir");
        std::fs::write(source.join("locked/note.txt"), "kept").expect("write file");
        std::fs::set_permissions(
            source.join("locked"),
            std::fs::Permissions::from_mode(0o555),
        )
        .expect("lock dir");
        let destination = dir.path().join("destination");

        let outcome = copy_and_remove(&source, &destination);

        std::fs::set_permissions(
            source.join("locked"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("unlock dir");
        assert!(matches!(outcome, Ok(Moved::SourceLeftBehind(_))));
        assert_eq!(
            std::fs::read_to_string(destination.join("locked/note.txt")).unwrap(),
            "kept"
        );
    }
}
