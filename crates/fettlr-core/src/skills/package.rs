use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::cli_records::CoreError;
use crate::skills::skill_files;

fn open_entry(path: &Path, key: &str, name: &str) -> Result<std::fs::File, CoreError> {
    let refuse = |reason: String| CoreError::UnpackageableSkill {
        name: name.to_string(),
        reason,
    };
    let file = open_without_following(path).map_err(|error| {
        if error.raw_os_error() == Some(libc::ELOOP) {
            refuse(format!("{key} is a symbolic link"))
        } else {
            CoreError::from(error)
        }
    })?;
    if !file.metadata()?.is_file() {
        return Err(refuse(format!("{key} is not a regular file")));
    }
    Ok(file)
}

fn open_without_following(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

/// Zips the files `skill_files` selects under `root_name/` and replaces `destination` with the
/// result. Dotfiles and symlinks never enter the archive.
pub fn write_skill_zip(
    source: &Path,
    root_name: &str,
    destination: &Path,
) -> Result<(), CoreError> {
    let unpackageable = |reason: String| CoreError::UnpackageableSkill {
        name: root_name.to_string(),
        reason,
    };
    let set = skill_files(source)?;
    if !set.files.contains_key("SKILL.md") {
        return Err(unpackageable("it has no SKILL.md".into()));
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut writer = ZipWriter::new(temporary.as_file_mut());
        let base = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer
            .add_directory(format!("{root_name}/"), base)
            .map_err(|error| unpackageable(error.to_string()))?;
        for (key, path) in &set.files {
            let mut file = open_entry(path, key, root_name)?;
            let executable = file.metadata()?.permissions().mode() & 0o111 != 0;
            let options = base.unix_permissions(if executable { 0o755 } else { 0o644 });
            writer
                .start_file(format!("{root_name}/{key}"), options)
                .map_err(|error| unpackageable(error.to_string()))?;
            std::io::copy(&mut file, &mut writer)?;
        }
        writer
            .finish()
            .map_err(|error| unpackageable(error.to_string()))?;
    }
    temporary.as_file_mut().flush()?;
    temporary
        .persist(destination)
        .map_err(|error| CoreError::from(error.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{open_entry, open_without_following};
    use crate::cli_records::CoreError;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn a_symlink_is_refused_by_the_open_helper() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real = dir.path().join("real.md");
        std::fs::write(&real, "x").expect("write");
        let link = dir.path().join("link.md");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");

        assert!(open_without_following(&real).is_ok());
        assert!(open_without_following(&link).is_err());
    }

    #[test]
    fn a_swapped_in_symlink_names_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real = dir.path().join("real.md");
        std::fs::write(&real, "x").expect("write");
        let link = dir.path().join("link.md");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");

        let error = open_entry(&link, "reference/link.md", "demo").expect_err("refused");

        match error {
            CoreError::UnpackageableSkill { name, reason } => {
                assert_eq!(name, "demo");
                assert_eq!(reason, "reference/link.md is a symbolic link");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_fifo_is_refused_without_blocking() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fifo = dir.path().join("pipe");
        let c_path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).expect("path");
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) }, 0);

        let error = open_entry(&fifo, "pipe", "demo").expect_err("refused");

        match error {
            CoreError::UnpackageableSkill { reason, .. } => {
                assert_eq!(reason, "pipe is not a regular file");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
