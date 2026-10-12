use std::fs::File;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::civil_time;
use crate::cli_records::CoreError;

pub fn load_json_store<T>(path: &Path) -> Result<T, CoreError>
where
    T: DeserializeOwned + Default + Serialize,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = std::fs::read_to_string(path)?;
    match serde_json::from_str::<T>(&raw) {
        Ok(value) => Ok(value),
        Err(error) => Err(rebuild_after_corruption::<T>(path, error.to_string())),
    }
}

pub fn save_json_store<T: Serialize>(path: &Path, value: &T) -> Result<(), CoreError> {
    // A rename replaces the link, not what it points at, so a config someone keeps symlinked into
    // a dotfiles repo would quietly become a regular file here and stop tracking. Resolving first
    // also keeps the temp file on the filesystem the rename has to land on.
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path = resolved.as_path();
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(value)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut temp, json.as_bytes())?;
    restrict_to_owner(temp.as_file())?;
    // The rename is atomic, the writeback behind it is not: without this a crash can leave the
    // renamed file empty, and one of the files written here is the user's whole ~/.claude.json.
    temp.as_file().sync_all()?;
    temp.persist(path)
        .map_err(|error| CoreError::Io(error.error))?;
    Ok(())
}

/// Fail-closed: the corrupt file is never discarded until it has been copied aside, and a copy
/// that cannot be made leaves the original completely untouched.
fn rebuild_after_corruption<T: Serialize + Default>(path: &Path, reason: String) -> CoreError {
    let backup = corrupt_backup_path(path);
    if std::fs::copy(path, &backup).is_ok() {
        let _ = std::fs::remove_file(path);
        let _ = save_json_store(path, &T::default());
        CoreError::CorruptStore {
            path: path.display().to_string(),
            reason,
            backup: Some(backup.display().to_string()),
        }
    } else {
        CoreError::CorruptStore {
            path: path.display().to_string(),
            reason,
            backup: None,
        }
    }
}

// A disabled MCP server's whole config, secrets included, is written through here, so these files
// never widen past the owner even when an earlier build left one group- and world-readable.
#[cfg(unix)]
fn restrict_to_owner(file: &File) -> Result<(), CoreError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_to_owner(_file: &File) -> Result<(), CoreError> {
    Ok(())
}

fn corrupt_backup_path(path: &Path) -> PathBuf {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("store.json");
    let stamp = civil_time::format_compact(civil_time::now_unix_seconds());
    let base = format!("{file_name}.corrupt-{stamp}");

    let mut candidate = dir.join(&base);
    let mut attempt = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base}-{attempt}"));
        attempt += 1;
    }
    candidate
}
