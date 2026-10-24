#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dto;
mod prefs;
mod state;
mod updater;

use std::path::PathBuf;
use std::sync::Arc;

use fettlr_core::Watcher;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_specta::{Event, collect_commands, collect_events};

use commands::{StateChanged, UpdateProgress};
use dto::UiSnapshot;
use state::AppState;
use updater::{AppUpdateProgress, PendingUpdate};

const SUPPORT_DIR_NAME: &str = "Fettlr";

/// Passed to the login item so a launch the reader did not ask for stays out of their way. Anything
/// else on the command line means a launch they did ask for.
const AUTOSTART_FLAG: &str = "--autostart";

const BINDINGS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::refresh,
            commands::check_updates,
            commands::park_skill,
            commands::restore_skill,
            commands::delete_skill,
            commands::delete_parked_skill,
            commands::pull_skill,
            commands::package_skill,
            commands::park_subagent,
            commands::restore_subagent,
            commands::delete_subagent,
            commands::delete_parked_subagent,
            commands::install_plugin,
            commands::uninstall_plugin,
            commands::set_plugin_enabled,
            commands::update_plugin,
            commands::add_marketplace,
            commands::remove_marketplace,
            commands::update_marketplace,
            commands::add_mcp_server,
            commands::edit_mcp_server,
            commands::remove_mcp_server,
            commands::promote_mcp_server,
            commands::set_mcp_server_enabled,
            commands::reveal_secret,
            commands::update_all,
            commands::plugin_details,
            commands::refresh_connectors,
            commands::mcp_details,
            commands::reveal_in_finder,
            commands::open_path,
            commands::get_prefs,
            commands::set_prefs,
            commands::probe_binary,
            commands::app_version,
            commands::open_url,
            updater::check_app_update,
            updater::install_app_update,
            updater::relaunch_app,
        ])
        .events(collect_events![
            StateChanged,
            UpdateProgress,
            AppUpdateProgress
        ])
        .error_handling(tauri_specta::ErrorHandlingMode::Result)
}

fn main() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    builder
        .export(specta_typescript::Typescript::default(), BINDINGS)
        .expect("could not export the TypeScript bindings");

    tauri::Builder::default()
        .plugin(logging())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_FLAG]),
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let state = Arc::new(AppState::new(claude_dir(), claude_json(), support_dir()));
            // Shown before the fallible watcher setup below: Tauri panics if this closure returns
            // Err, and a normal launch has to show the window even when that panic is coming.
            if !launched_by_login_item() {
                show_main_window(app.handle());
            }
            start_watcher(app.handle().clone(), &state)?;
            state.attach(app.handle().clone());
            app.manage(state);
            app.manage(PendingUpdate::default());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Fettlr could not start")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main_window(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

/// Warnings and errors only in a release build: everything Fettlr logs is a failure, and the lines
/// carry a message and, at most, the stderr of a command whose arguments the core already redacts.
fn logging<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    };
    tauri_plugin_log::Builder::new()
        .level(level)
        .targets([
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: None }),
        ])
        .build()
}

fn launched_by_login_item() -> bool {
    std::env::args().any(|arg| arg == AUTOSTART_FLAG)
}

/// The window is created hidden (`"visible": false` in `tauri.conf.json`) so a login launch never
/// flashes on screen; a normal launch has to show it itself.
fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("the main window was gone before it could be shown");
        return;
    };
    if let Err(error) = window.show() {
        log::warn!("could not show the main window: {error}");
    }
    if let Err(error) = window.set_focus() {
        log::warn!("could not focus the main window: {error}");
    }
}

/// The watcher's throttle already coalesces a burst of changes into one callback; the refresh it
/// runs is the same one the Refresh button runs, so the window converges on the same snapshot
/// whichever set it off. The callback holds a `Weak`, because the state owns the watcher and an
/// `Arc` here would be a cycle that keeps both alive for the life of the process.
fn start_watcher(app: AppHandle, state: &Arc<AppState>) -> Result<(), fettlr_core::CoreError> {
    let watched = Arc::downgrade(state);
    let watcher = Watcher::start(
        &state.watch_paths(),
        Box::new(move || {
            let Some(state) = watched.upgrade() else {
                return;
            };
            match state.fettlr().refresh() {
                Ok(snapshot) => {
                    if let Err(error) = StateChanged(UiSnapshot::from(snapshot)).emit(&app) {
                        log::warn!("could not publish the refreshed state: {error}");
                    }
                }
                Err(error) => log::warn!("a background refresh failed: {error}"),
            }
        }),
    )?;
    state.hold_watcher(watcher);
    Ok(())
}

fn claude_dir() -> PathBuf {
    home().join(".claude")
}

fn claude_json() -> PathBuf {
    home().join(".claude.json")
}

/// The same location the Swift build used, so parked skills and disabled MCP servers carry over
/// rather than being rediscovered as missing.
fn support_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| home().join("Library").join("Application Support"))
        .join(SUPPORT_DIR_NAME)
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_bindings_match_the_commands() {
        let scratch = tempfile::tempdir().unwrap();
        let exported = scratch.path().join("bindings.ts");
        specta_builder()
            .export(specta_typescript::Typescript::default(), &exported)
            .unwrap();
        let fresh = std::fs::read_to_string(&exported).unwrap();
        let committed = std::fs::read_to_string(BINDINGS).unwrap();
        assert!(
            fresh == committed,
            "app/src/bindings.ts is stale: run `pnpm tauri dev` once and commit bindings.ts"
        );
    }

    #[test]
    fn the_shipped_layout_hangs_off_the_real_home() {
        assert_eq!(claude_dir(), home().join(".claude"));
        assert_eq!(claude_json(), home().join(".claude.json"));
        assert_eq!(claude_dir().parent(), Some(home().as_path()));
    }

    #[test]
    fn the_support_dir_is_fettlrs_own_folder() {
        let support = support_dir();
        assert_eq!(support.file_name(), Some("Fettlr".as_ref()));
        assert_ne!(support.parent(), Some(claude_dir().as_path()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_support_dir_sits_in_application_support() {
        assert_eq!(
            support_dir(),
            home()
                .join("Library")
                .join("Application Support")
                .join("Fettlr")
        );
    }
}
