use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::cli_records::CoreError;
use crate::json_store::save_json_store;
use crate::paths::Paths;
use crate::trashing::Trashing;

/// Where the skills CLI recorded a skill coming from, decoded from `~/.agents/.skill-lock.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SkillsCliOrigin {
    pub source: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub reference: Option<String>,
    pub folder: String,
    pub folder_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillsLock {
    Missing,
    Unsupported { version: String },
    Read(BTreeMap<String, SkillsCliOrigin>),
}

fn is_version_three(root: &Value) -> bool {
    root.get("version").and_then(Value::as_u64) == Some(3)
}

fn describe_version(root: &Value) -> String {
    match root.get("version") {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => "unknown".to_string(),
    }
}

fn decode_entry(entry: &Value) -> Option<SkillsCliOrigin> {
    let text = |key: &str| entry.get(key).and_then(Value::as_str);
    let skill_path = text("skillPath")?;
    let folder = match skill_path.rsplit_once('/') {
        Some((".", _)) | None => "",
        Some((folder, _)) => folder,
    };
    Some(SkillsCliOrigin {
        source: text("source")?.to_string(),
        source_type: text("sourceType")?.to_string(),
        source_url: text("sourceUrl").map(str::to_string),
        reference: text("ref")
            .filter(|reference| !reference.is_empty())
            .map(str::to_string),
        folder: folder.to_string(),
        folder_hash: text("skillFolderHash")?.to_string(),
    })
}

/// Reads the lock without ever rewriting it: a lock Fettlr cannot parse belongs to the skills
/// CLI, so it is reported and left exactly as found.
pub fn read_skills_lock(path: &Path) -> Result<SkillsLock, CoreError> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SkillsLock::Missing);
        }
        Err(error) => return Err(error.into()),
    };
    let root = serde_json::from_str::<Value>(&raw)?;
    if !is_version_three(&root) {
        return Ok(SkillsLock::Unsupported {
            version: describe_version(&root),
        });
    }
    let entries = root
        .get("skills")
        .and_then(Value::as_object)
        .map(|skills| {
            skills
                .iter()
                .filter_map(|(name, entry)| Some((name.clone(), decode_entry(entry)?)))
                .collect()
        })
        .unwrap_or_default();
    Ok(SkillsLock::Read(entries))
}

/// Re-reads the lock and changes only the two fields of one entry, so anything the skills CLI
/// wrote since Fettlr last looked survives. Refuses rather than guessing when the entry is gone.
pub fn record_skill_update(
    path: &Path,
    name: &str,
    folder_hash: &str,
    updated_at: &str,
) -> Result<(), CoreError> {
    let changed = || CoreError::SkillsLockChanged {
        path: path.display().to_string(),
    };
    let raw = std::fs::read_to_string(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            changed()
        } else {
            CoreError::Io(error)
        }
    })?;
    let mut root = serde_json::from_str::<Value>(&raw).map_err(|_| changed())?;
    if !is_version_three(&root) {
        return Err(changed());
    }
    let entry = root
        .get_mut("skills")
        .and_then(|skills| skills.get_mut(name))
        .and_then(Value::as_object_mut)
        .ok_or_else(changed)?;
    entry.insert("skillFolderHash".into(), folder_hash.into());
    entry.insert("updatedAt".into(), updated_at.into());
    save_json_store(path, &root)
}

/// Swaps `staged` in at `target` by two renames, keeping the old folder at `set_aside` until the
/// new one is in place. A failed second rename moves the old folder back. The old copy is trashed
/// last, and a failure to trash it is only logged: the update itself has already landed.
pub fn replace_skill_folder(
    staged: &Path,
    target: &Path,
    set_aside: &Path,
    trasher: &dyn Trashing,
) -> Result<(), CoreError> {
    if let Some(parent) = set_aside.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(target, set_aside)?;
    if let Err(error) = std::fs::rename(staged, target) {
        if let Err(restore) = std::fs::rename(set_aside, target) {
            log::warn!(
                "could not move {} back to {}: {restore}",
                set_aside.display(),
                target.display()
            );
        }
        return Err(error.into());
    }
    if let Err(error) = trasher.trash(set_aside) {
        log::warn!("{error}");
    }
    Ok(())
}

pub(crate) fn is_plain_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(_)), None)
    )
}

const SET_ASIDE_DIR: &str = ".fettlr-replaced";
const LEGACY_SET_ASIDE_PREFIX: &str = ".fettlr-replaced-";

/// Where an update parks a skill's old folder while the new one is swapped in. The folder keeps
/// the skill's own name, so the copy that lands in the Trash is recognizable.
pub(crate) fn set_aside_path(paths: &Paths, name: &str) -> PathBuf {
    paths.skills_cli_dir().join(SET_ASIDE_DIR).join(name)
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

/// Moves back every skill folder an interrupted update left set aside while nothing has taken its
/// place. Never trashes or deletes anything: the set-aside copy may be the only one left.
pub fn recover_interrupted_swaps(paths: &Paths) {
    let cli_dir = paths.skills_cli_dir();
    let set_aside_dir = cli_dir.join(SET_ASIDE_DIR);
    if is_real_dir(&set_aside_dir)
        && let Ok(entries) = std::fs::read_dir(&set_aside_dir)
    {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                restore_set_aside(paths, name, &entry.path());
            }
        }
    }
    let Ok(entries) = std::fs::read_dir(&cli_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        if let Some(name) = file_name
            .to_str()
            .and_then(|text| text.strip_prefix(LEGACY_SET_ASIDE_PREFIX))
        {
            restore_set_aside(paths, name, &entry.path());
        }
    }
}

fn restore_set_aside(paths: &Paths, name: &str, set_aside: &Path) {
    if !is_plain_name(name) || !is_real_dir(set_aside) {
        return;
    }
    let target = paths.skills_cli_skills_dir().join(name);
    let vacant = matches!(
        std::fs::symlink_metadata(&target),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    if !vacant {
        return;
    }
    if let Err(error) = std::fs::rename(set_aside, &target) {
        log::warn!(
            "could not move {} back to {}: {error}",
            set_aside.display(),
            target.display()
        );
    }
}
