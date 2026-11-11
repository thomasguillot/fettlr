use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

const LEFTOVER_PREFIXES: &[&str] = &["tmp_pack_", "tmp_idx_", "tmp_rev_"];
const MIN_AGE: Duration = Duration::from_secs(60 * 60);

/// Removes the temporary pack files a killed fetch leaves behind in a clone's `.git`. Best effort:
/// every failure is ignored.
pub(crate) fn sweep_clone(clone: &Path) {
    let git_dir = clone.join(".git");
    if is_real_dir(&git_dir) {
        sweep_git_dir(&git_dir);
    }
}

/// Same as `sweep_clone`, for every bare repository directly under Fettlr's skill source cache.
pub(crate) fn sweep_source_cache(cache: &Path) {
    if !is_real_dir(cache) {
        return;
    }
    let Ok(entries) = fs::read_dir(cache) else {
        return;
    };
    for entry in entries.flatten() {
        let repo = entry.path();
        if is_real_dir(&repo) {
            sweep_git_dir(&repo);
        }
    }
}

fn sweep_git_dir(git_dir: &Path) {
    let objects = git_dir.join("objects");
    let pack = objects.join("pack");
    if !is_real_dir(&objects) || !is_real_dir(&pack) {
        return;
    }
    let Ok(entries) = fs::read_dir(&pack) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !LEFTOVER_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let old_enough = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > MIN_AGE);
        if old_enough {
            let _ = fs::remove_file(&path);
        }
    }
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}
