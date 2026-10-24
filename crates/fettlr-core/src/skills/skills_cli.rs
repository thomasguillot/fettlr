use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::cli_records::CoreError;
use crate::json_store::save_json_store;

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
    let folder = skill_path
        .rsplit_once('/')
        .map(|(folder, _)| folder)
        .unwrap_or("");
    Some(SkillsCliOrigin {
        source: text("source")?.to_string(),
        source_type: text("sourceType")?.to_string(),
        source_url: text("sourceUrl").map(str::to_string),
        reference: text("ref").map(str::to_string),
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
    let raw = std::fs::read_to_string(path).map_err(|_| changed())?;
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
