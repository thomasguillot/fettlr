use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

use crate::dto::UiError;

/// The release the last check found, kept so that installing does not have to ask the endpoint a
/// second time and risk downloading a different build from the one the reader agreed to.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

impl PendingUpdate {
    fn store(&self, update: Option<Update>) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = update;
    }

    /// A copy, deliberately: an install that fails has to leave the release pending so the reader
    /// can try again without checking the endpoint a second time.
    fn current(&self) -> Option<Update> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiAppUpdate {
    pub version: String,
    pub notes: Option<String>,
}

impl From<&Update> for UiAppUpdate {
    fn from(update: &Update) -> Self {
        Self {
            version: update.version.clone(),
            notes: update.body.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateProgress {
    pub downloaded: u32,
    /// Absent while the server has not said how large the package is.
    pub total: Option<u32>,
}

/// Answers `None` for "nothing newer is published" and an error for "the question could not be
/// asked": an endpoint that is unreachable, unparseable, or not there yet must leave the window
/// with something to say rather than take the app down with it.
#[tauri::command]
#[specta::specta]
pub async fn check_app_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UiAppUpdate>, UiError> {
    let found = app
        .updater()
        .map_err(check_failed)?
        .check()
        .await
        .map_err(check_failed)?;
    let answer = found.as_ref().map(UiAppUpdate::from);
    pending.store(found);
    Ok(answer)
}

/// Downloads and installs the release the last check found. The plugin verifies the package
/// signature before it writes anything, and a package that fails that check is refused here too.
#[tauri::command]
#[specta::specta]
pub async fn install_app_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<(), UiError> {
    let update = pending.current().ok_or_else(|| {
        UiError::message("Fettlr has not found an update to install. Check again first.")
    })?;

    let mut downloaded: u64 = 0;
    let reporter = app.clone();
    update
        .download_and_install(
            |chunk, length| {
                downloaded += chunk as u64;
                let progress = AppUpdateProgress {
                    downloaded: bytes(downloaded),
                    total: length.map(bytes),
                };
                if let Err(error) = progress.emit(&reporter) {
                    log::warn!("could not report the download progress: {error}");
                }
            },
            || {},
        )
        .await
        .map_err(|error| UiError {
            message: "Fettlr could not install the update.".to_string(),
            detail: Some(error.to_string()),
        })?;
    pending.store(None);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn relaunch_app(app: AppHandle) -> Result<(), UiError> {
    app.restart()
}

/// The window only ever divides these two into a fraction, and no Fettlr package will come close
/// to four gigabytes, so a saturating cast costs a truthful bar nothing.
fn bytes(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

fn check_failed(error: tauri_plugin_updater::Error) -> UiError {
    log::warn!("could not check for a Fettlr update: {error}");
    UiError {
        message: "Fettlr could not check for updates.".to_string(),
        detail: Some(error.to_string()),
    }
}
