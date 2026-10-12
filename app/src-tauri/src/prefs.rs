use std::path::{Path, PathBuf};

use fettlr_core::{CoreError, load_json_store, save_json_store};
use serde::{Deserialize, Serialize};

use crate::dto::{UiColorMode, UiPalette, UiPrefs};

const DEFAULT_CHECK_INTERVAL_HOURS: u32 = 6;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub binary_override: Option<String>,
    pub check_interval_hours: Option<u32>,
    pub auto_check_for_updates: bool,
    #[serde(alias = "theme")]
    pub color_mode: UiColorMode,
    #[serde(default, deserialize_with = "lenient_palette")]
    pub palette: UiPalette,
    pub launch_at_login: bool,
    pub confirm_mcp_promotion: bool,
    pub sidebar_collapsed: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            binary_override: None,
            check_interval_hours: Some(DEFAULT_CHECK_INTERVAL_HOURS),
            auto_check_for_updates: true,
            color_mode: UiColorMode::System,
            palette: UiPalette::default(),
            launch_at_login: false,
            confirm_mcp_promotion: true,
            sidebar_collapsed: false,
        }
    }
}

impl Prefs {
    pub fn binary_override_path(&self) -> Option<PathBuf> {
        self.binary_override
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
    }
}

impl From<&Prefs> for UiPrefs {
    fn from(prefs: &Prefs) -> Self {
        Self {
            binary_override: prefs.binary_override.clone(),
            check_interval_hours: prefs.check_interval_hours,
            auto_check_for_updates: prefs.auto_check_for_updates,
            color_mode: prefs.color_mode,
            palette: prefs.palette,
            launch_at_login: prefs.launch_at_login,
            confirm_mcp_promotion: prefs.confirm_mcp_promotion,
            sidebar_collapsed: prefs.sidebar_collapsed,
        }
    }
}

impl From<&UiPrefs> for Prefs {
    fn from(prefs: &UiPrefs) -> Self {
        Self {
            binary_override: prefs
                .binary_override
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            check_interval_hours: prefs.check_interval_hours.filter(|hours| *hours > 0),
            auto_check_for_updates: prefs.auto_check_for_updates,
            color_mode: prefs.color_mode,
            palette: prefs.palette,
            launch_at_login: prefs.launch_at_login,
            confirm_mcp_promotion: prefs.confirm_mcp_promotion,
            sidebar_collapsed: prefs.sidebar_collapsed,
        }
    }
}

/// A palette value the running build no longer defines (a variant removed since the file was
/// written) must not fail deserialization of the whole `Prefs` struct: `load_json_store` treats a
/// parse failure as corruption and rebuilds the store from defaults, which would discard every
/// other preference in the file along with the stale palette.
fn lenient_palette<'de, D>(deserializer: D) -> Result<UiPalette, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

fn store_path(support_dir: &Path) -> PathBuf {
    support_dir.join("preferences.json")
}

pub fn load(support_dir: &Path) -> Result<Prefs, CoreError> {
    load_json_store(&store_path(support_dir))
}

pub fn save(support_dir: &Path, prefs: &Prefs) -> Result<(), CoreError> {
    save_json_store(&store_path(support_dir), prefs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_garbage(dir: &Path) -> &'static str {
        const GARBAGE: &str = "{ not a set of preferences";
        std::fs::create_dir_all(dir).expect("create support dir");
        std::fs::write(store_path(dir), GARBAGE).expect("write garbage");
        GARBAGE
    }

    #[test]
    fn the_defaults_match_the_shipped_app() {
        let prefs = Prefs::default();
        assert_eq!(prefs.check_interval_hours, Some(6));
        assert!(prefs.auto_check_for_updates);
        assert_eq!(prefs.binary_override_path(), None);
        assert_eq!(prefs.color_mode, UiColorMode::System);
        assert_eq!(prefs.palette, UiPalette::Default);
        assert!(!prefs.launch_at_login);
        assert!(prefs.confirm_mcp_promotion);
        assert!(!prefs.sidebar_collapsed);
    }

    #[test]
    fn a_missing_file_reads_as_the_defaults() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        assert_eq!(load(dir.path()).expect("loads"), Prefs::default());
    }

    #[test]
    fn saved_prefs_come_back_unchanged() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let prefs = Prefs {
            binary_override: Some("/opt/bin/claude".to_string()),
            check_interval_hours: None,
            auto_check_for_updates: false,
            color_mode: UiColorMode::Dark,
            palette: UiPalette::Dracula,
            launch_at_login: true,
            confirm_mcp_promotion: false,
            sidebar_collapsed: true,
        };
        save(dir.path(), &prefs).expect("saves");
        assert_eq!(load(dir.path()).expect("loads"), prefs);
    }

    #[test]
    fn a_partial_file_fills_the_rest_from_the_defaults() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(dir.path()).expect("create support dir");
        std::fs::write(store_path(dir.path()), r#"{"autoCheckForUpdates": false}"#)
            .expect("write partial");
        let prefs = load(dir.path()).expect("loads");
        assert!(!prefs.auto_check_for_updates);
        assert_eq!(prefs.check_interval_hours, Some(6));
        assert_eq!(prefs.color_mode, UiColorMode::System);
    }

    #[test]
    fn a_blank_binary_override_is_treated_as_unset() {
        let prefs = Prefs::from(&UiPrefs {
            binary_override: Some("   ".to_string()),
            check_interval_hours: Some(0),
            ..UiPrefs::from(&Prefs::default())
        });
        assert_eq!(prefs.binary_override, None);
        assert_eq!(prefs.check_interval_hours, None);
    }

    #[test]
    fn a_store_still_holding_hidden_projects_loads_the_other_values() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(dir.path()).expect("create support dir");
        std::fs::write(
            store_path(dir.path()),
            r#"{"hiddenProjects":["/tmp/project"],"colorMode":"dark","checkIntervalHours":12}"#,
        )
        .expect("write legacy prefs");

        let prefs = load(dir.path()).expect("loads");
        assert_eq!(prefs.color_mode, UiColorMode::Dark);
        assert_eq!(prefs.check_interval_hours, Some(12));
    }

    #[test]
    fn an_unreadable_store_is_copied_aside_and_rebuilt() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let garbage = write_garbage(dir.path());

        let error = load(dir.path()).expect_err("refuses the garbage");

        let CoreError::CorruptStore {
            path,
            reason,
            backup,
        } = error
        else {
            panic!("expected a corrupt-store error, got {error}");
        };
        let backup = backup.expect("the unreadable file was copied aside");
        assert_eq!(path, store_path(dir.path()).display().to_string());
        assert!(!reason.is_empty());
        assert!(
            Path::new(&backup)
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a backup file name")
                .starts_with("preferences.json.corrupt-")
        );
        assert_eq!(
            std::fs::read_to_string(&backup).expect("read backup"),
            garbage
        );
        assert_eq!(load(dir.path()).expect("loads"), Prefs::default());
    }

    #[test]
    fn an_unreadable_store_that_cannot_be_copied_aside_is_left_as_it_is() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::TempDir::new().expect("tempdir");
        let garbage = write_garbage(dir.path());
        let readable = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
            .expect("make the support dir read-only");

        let error = load(dir.path());
        std::fs::set_permissions(dir.path(), readable).expect("restore the support dir");

        let CoreError::CorruptStore { reason, backup, .. } =
            error.expect_err("refuses the garbage")
        else {
            panic!("expected a corrupt-store error");
        };
        assert_eq!(backup, None);
        assert!(!reason.is_empty());
        assert_eq!(
            std::fs::read_to_string(store_path(dir.path())).expect("read store"),
            garbage
        );
    }

    #[test]
    fn a_store_still_written_as_theme_keeps_its_color_mode() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(dir.path()).expect("create support dir");
        std::fs::write(store_path(dir.path()), r#"{"theme":"dark"}"#).expect("write legacy prefs");

        let prefs = load(dir.path()).expect("loads");
        assert_eq!(prefs.color_mode, UiColorMode::Dark);
        assert_eq!(prefs.palette, UiPalette::Default);
    }

    #[test]
    fn a_non_default_palette_survives_a_reload() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let prefs = Prefs {
            palette: UiPalette::HighContrast,
            ..Prefs::default()
        };
        save(dir.path(), &prefs).expect("saves");
        assert_eq!(load(dir.path()).expect("loads"), prefs);
    }

    #[test]
    fn an_unrecognized_palette_falls_back_to_the_default_without_losing_the_rest_of_the_file() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(dir.path()).expect("create support dir");
        std::fs::write(
            store_path(dir.path()),
            r#"{"palette":"custom","colorMode":"dark","checkIntervalHours":12}"#,
        )
        .expect("write prefs with a removed palette variant");

        let prefs = load(dir.path()).expect("loads");
        assert_eq!(prefs.palette, UiPalette::Default);
        assert_eq!(prefs.color_mode, UiColorMode::Dark);
        assert_eq!(prefs.check_interval_hours, Some(12));
    }
}
