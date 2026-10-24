use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cli_records::CoreError;

const MAX_DEPTH: usize = 16;
const MAX_FILES: usize = 5000;

pub(crate) struct SkillFiles {
    pub files: BTreeMap<String, PathBuf>,
}

pub(crate) fn skill_files(root: &Path) -> Result<SkillFiles, CoreError> {
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let canonical = std::fs::canonicalize(root)?;
    let mut files = BTreeMap::new();
    walk(&canonical, "", 1, &name, &mut files)?;
    Ok(SkillFiles { files })
}

fn walk(
    directory: &Path,
    prefix: &str,
    depth: usize,
    name: &str,
    files: &mut BTreeMap<String, PathBuf>,
) -> Result<(), CoreError> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.starts_with('.') {
            continue;
        }
        let file_type = entry.file_type()?;
        let key = format!("{prefix}{file_name}");
        if file_type.is_dir() {
            if depth >= MAX_DEPTH {
                return Err(too_large(name));
            }
            walk(&entry.path(), &format!("{key}/"), depth + 1, name, files)?;
        } else if file_type.is_file() {
            if files.len() >= MAX_FILES {
                return Err(too_large(name));
            }
            files.insert(key, entry.path());
        }
    }
    Ok(())
}

fn too_large(name: &str) -> CoreError {
    CoreError::UnpackageableSkill {
        name: name.to_string(),
        reason: "it holds more than 5000 files or 16 levels of folders".into(),
    }
}
