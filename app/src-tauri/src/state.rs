use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use fettlr_core::{
    CoreError, Fettlr, GitProbe, HttpRegistryClient, Paths, ProcessCliRunner, Snapshot,
    SystemTrasher, Watcher,
};

use crate::prefs::{self, Prefs};

pub struct PrefsOutcome {
    pub prefs: Prefs,
    /// The snapshot the rebuilt handle read, present only when the binary override changed. A fresh
    /// `Fettlr` starts with an empty cache and nothing on disk has moved, so the watcher will not
    /// fire: without refilling it here, every command that resolves an id against the cache would
    /// answer "Fettlr no longer knows..." for rows still on screen.
    pub refreshed: Option<Snapshot>,
}

pub struct AppState {
    claude_dir: PathBuf,
    claude_json: PathBuf,
    support_dir: PathBuf,
    core: RwLock<Arc<Fettlr>>,
    prefs: Mutex<Prefs>,
    watcher: Mutex<Option<Watcher>>,
    /// Set once during setup. Lets a command publish state outside its own answer, which a failed
    /// mutation has to do: the core refreshes either way, so the window would otherwise be the
    /// only place still holding the world as it was before the command ran.
    app: OnceLock<tauri::AppHandle>,
}

impl AppState {
    pub fn new(claude_dir: PathBuf, claude_json: PathBuf, support_dir: PathBuf) -> Self {
        let prefs = prefs::load(&support_dir).unwrap_or_else(|error| {
            log::warn!("could not read the preferences, falling back to the defaults: {error}");
            Prefs::default()
        });
        let paths = build_paths(&claude_dir, &claude_json, &support_dir, &prefs);
        Self {
            claude_dir,
            claude_json,
            support_dir,
            core: RwLock::new(build_core(paths)),
            prefs: Mutex::new(prefs),
            watcher: Mutex::new(None),
            app: OnceLock::new(),
        }
    }

    pub fn attach(&self, app: tauri::AppHandle) {
        let _ = self.app.set(app);
    }

    pub fn app(&self) -> Option<&tauri::AppHandle> {
        self.app.get()
    }

    pub fn fettlr(&self) -> Arc<Fettlr> {
        Arc::clone(&self.read_core())
    }

    pub fn prefs(&self) -> Prefs {
        self.lock_prefs().clone()
    }

    /// Persists first, then repoints the core handle: a write that fails leaves both the file and
    /// the running handle on the settings the reader already had. The save and the swap run under
    /// the prefs lock so two writes cannot leave the file and the live handle disagreeing; the
    /// refresh that refills the new handle's cache runs after it, since it shells out to `claude`
    /// and nothing else should wait on the lock for that long.
    pub fn set_prefs(&self, next: Prefs) -> Result<PrefsOutcome, CoreError> {
        let rebuilt = {
            let mut current = self.lock_prefs();
            let rebuild = current.binary_override_path() != next.binary_override_path();
            prefs::save(&self.support_dir, &next)?;
            if rebuild {
                let paths = build_paths(
                    &self.claude_dir,
                    &self.claude_json,
                    &self.support_dir,
                    &next,
                );
                *self.write_core() = build_core(paths);
            }
            *current = next.clone();
            rebuild
        };

        let refreshed = if rebuilt {
            Some(self.fettlr().refresh()?)
        } else {
            None
        };
        Ok(PrefsOutcome {
            prefs: next,
            refreshed,
        })
    }

    pub fn watch_paths(&self) -> Paths {
        build_paths(
            &self.claude_dir,
            &self.claude_json,
            &self.support_dir,
            &self.prefs(),
        )
    }

    /// The same tree the live handle reads, pointed at a binary the reader has typed but not saved,
    /// so the settings indicator answers for the whole resolution order rather than for the
    /// override alone.
    pub fn paths_probing(&self, binary_override: Option<PathBuf>) -> Paths {
        Paths {
            claude_dir: self.claude_dir.clone(),
            claude_json: self.claude_json.clone(),
            support_dir: self.support_dir.clone(),
            binary_override,
        }
    }

    pub fn hold_watcher(&self, watcher: Watcher) {
        *self
            .watcher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(watcher);
    }

    fn read_core(&self) -> std::sync::RwLockReadGuard<'_, Arc<Fettlr>> {
        self.core
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_core(&self) -> std::sync::RwLockWriteGuard<'_, Arc<Fettlr>> {
        self.core
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_prefs(&self) -> std::sync::MutexGuard<'_, Prefs> {
        self.prefs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn build_paths(claude_dir: &Path, claude_json: &Path, support_dir: &Path, prefs: &Prefs) -> Paths {
    Paths {
        claude_dir: claude_dir.to_path_buf(),
        claude_json: claude_json.to_path_buf(),
        support_dir: support_dir.to_path_buf(),
        binary_override: prefs.binary_override_path(),
    }
}

fn build_core(paths: Paths) -> Arc<Fettlr> {
    Arc::new(Fettlr::new(
        paths,
        Arc::new(ProcessCliRunner),
        Arc::new(GitProbe),
        Arc::new(HttpRegistryClient),
        Arc::new(SystemTrasher),
    ))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// A binary that answers every call successfully with empty output: enough for the override to
    /// be accepted and for a refresh to complete, without ever reaching the real `claude`.
    fn stub_binary(dir: &Path, name: &str) -> PathBuf {
        let binary = dir.join(name);
        std::fs::write(&binary, "#!/bin/sh\nexit 0\n").expect("write stub");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .expect("make the stub executable");
        binary
    }

    fn state_at(dir: &Path) -> AppState {
        AppState::new(
            dir.join(".claude"),
            dir.join(".claude.json"),
            dir.join("Support"),
        )
    }

    fn prefs_pointing_at(binary: &Path) -> Prefs {
        Prefs {
            binary_override: Some(binary.display().to_string()),
            ..Prefs::default()
        }
    }

    #[test]
    fn a_new_handle_starts_with_an_empty_cache() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = state_at(dir.path());
        assert!(state.fettlr().snapshot().last_refreshed.is_none());
    }

    /// The rebuilt handle's cache is what every id-addressed command resolves against, and nothing
    /// on disk moved, so the watcher will not refill it. Without this the window would keep showing
    /// rows that every command then claims not to know.
    #[test]
    fn changing_the_binary_override_leaves_the_new_handle_with_a_read_cache() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = state_at(dir.path());
        let binary = stub_binary(dir.path(), "claude-one");

        let outcome = state
            .set_prefs(prefs_pointing_at(&binary))
            .expect("saves and refreshes");

        assert!(outcome.refreshed.is_some());
        assert!(state.fettlr().snapshot().last_refreshed.is_some());
    }

    #[test]
    fn changing_the_binary_override_repoints_the_handle() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = state_at(dir.path());
        let first = state.fettlr();

        let binary = stub_binary(dir.path(), "claude-one");
        state.set_prefs(prefs_pointing_at(&binary)).expect("saves");

        assert!(!Arc::ptr_eq(&first, &state.fettlr()));
        assert_eq!(
            state.prefs().binary_override_path(),
            Some(binary.clone()),
            "the saved prefs point at the new binary"
        );
    }

    #[test]
    fn a_settings_change_that_leaves_the_binary_alone_keeps_the_handle_and_its_cache() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = state_at(dir.path());
        let binary = stub_binary(dir.path(), "claude-one");
        state.set_prefs(prefs_pointing_at(&binary)).expect("saves");
        let handle = state.fettlr();

        let outcome = state
            .set_prefs(Prefs {
                auto_check_for_updates: false,
                ..prefs_pointing_at(&binary)
            })
            .expect("saves");

        assert!(
            outcome.refreshed.is_none(),
            "nothing was invalidated, so nothing needed re-reading"
        );
        assert!(Arc::ptr_eq(&handle, &state.fettlr()));
        assert!(!state.prefs().auto_check_for_updates);
    }

    #[test]
    fn clearing_the_binary_override_also_refills_the_cache() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = state_at(dir.path());
        let binary = stub_binary(dir.path(), "claude-one");
        state.set_prefs(prefs_pointing_at(&binary)).expect("saves");

        let outcome = state.set_prefs(Prefs::default()).expect("saves");

        assert!(outcome.refreshed.is_some());
        assert_eq!(state.prefs().binary_override_path(), None);
    }

    #[test]
    fn the_saved_prefs_survive_a_restart() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let binary = stub_binary(dir.path(), "claude-one");
        state_at(dir.path())
            .set_prefs(prefs_pointing_at(&binary))
            .expect("saves");

        assert_eq!(
            state_at(dir.path()).prefs().binary_override_path(),
            Some(binary)
        );
    }
}
