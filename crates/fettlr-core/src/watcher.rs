use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use notify::Watcher as _;
use notify::{Event, RecommendedWatcher, RecursiveMode};

use crate::cli_records::CoreError;
use crate::paths::Paths;

const DEFAULT_THROTTLE: Duration = Duration::from_secs(10);

pub struct Watcher {
    _inner: RecommendedWatcher,
    stopped: Arc<AtomicBool>,
}

impl Watcher {
    pub fn start(paths: &Paths, on_change: Box<dyn Fn() + Send>) -> Result<Watcher, CoreError> {
        Self::with_throttle(paths, on_change, DEFAULT_THROTTLE)
    }

    /// Same as `start`, with an injectable throttle window so tests don't wait out the real
    /// 10-second default.
    pub fn with_throttle(
        paths: &Paths,
        on_change: Box<dyn Fn() + Send>,
        throttle: Duration,
    ) -> Result<Watcher, CoreError> {
        let watched = interesting_paths(paths);
        let stopped = Arc::new(AtomicBool::new(false));
        let throttler = Arc::new(Throttler::new(throttle, on_change, Arc::clone(&stopped)));
        let handler_watched = watched.clone();
        let handler_throttler = Arc::clone(&throttler);

        let mut inner = notify::recommended_watcher(move |result: notify::Result<Event>| {
            let event = match result {
                Ok(event) => event,
                Err(error) => {
                    log::warn!("file watcher reported an error: {error}");
                    return;
                }
            };
            let interesting = event.paths.is_empty()
                || event
                    .paths
                    .iter()
                    .any(|path| is_of_interest(path, &handler_watched));
            if interesting {
                handler_throttler.trigger();
            }
        })
        .map_err(|error| watch_error(&paths.claude_dir, &error))?;

        for root in watch_roots(paths) {
            inner
                .watch(&root, RecursiveMode::Recursive)
                .map_err(|error| watch_error(&root, &error))?;
        }

        Ok(Watcher {
            _inner: inner,
            stopped,
        })
    }
}

impl Drop for Watcher {
    /// A throttle window already in flight (a spawned thread sleeping out its wait) has no other
    /// way to learn the watcher went away, since it holds its own `Arc` clone of everything it
    /// needs. This is the Rust equivalent of the Swift watcher's `[weak self]` task: without it, a
    /// change observed just before drop would still call into whatever the callback closes over
    /// up to a whole throttle window later.
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
    }
}

fn watch_error(path: &Path, error: &notify::Error) -> CoreError {
    CoreError::Watch {
        path: path.display().to_string(),
        reason: error.to_string(),
    }
}

/// What a change is measured against: FSEvents-style, an ancestor of a watched entry counts too,
/// since a coalesced burst may only report the shallower directory.
fn is_of_interest(path: &Path, watched: &[PathBuf; 6]) -> bool {
    watched
        .iter()
        .any(|entry| path.starts_with(entry) || entry.starts_with(path))
}

/// Claude Code writes transcripts, sessions and backups under the same tree roughly every minute,
/// and a refresh shells out to `claude` twice and re-walks every project, so everything else is
/// ignored. Resolved (`/var` -> `/private/var` on macOS, and similarly for a symlinked home) since
/// that is the form the OS reports changed paths in, even when the original was not resolved.
fn interesting_paths(paths: &Paths) -> [PathBuf; 6] {
    let plugins_dir = paths.plugins_dir();
    [
        resolve_best_effort(&paths.claude_json),
        resolve_best_effort(&paths.skills_dir()),
        resolve_best_effort(&paths.agents_dir()),
        resolve_best_effort(&plugins_dir.join("installed_plugins.json")),
        resolve_best_effort(&plugins_dir.join("known_marketplaces.json")),
        resolve_best_effort(&paths.settings_json()),
    ]
}

/// `std::fs::canonicalize` requires the path to exist, which a path Fettlr watches may not yet:
/// this resolves as much of it as does exist (its nearest existing ancestor) and reattaches the
/// rest unchanged, which is enough to match what the OS reports once something changes there.
fn resolve_best_effort(path: &Path) -> PathBuf {
    let ancestor = existing_ancestor(path);
    let Ok(suffix) = path.strip_prefix(&ancestor) else {
        return path.to_path_buf();
    };
    std::fs::canonicalize(&ancestor)
        .unwrap_or(ancestor)
        .join(suffix)
}

/// What gets registered with the OS watcher. `plugins_dir` stands in for the two files inside it
/// that `is_of_interest` actually cares about: FSEvents (and `notify`'s backend for it) watches
/// whole trees, so the root is what is registered and the callback narrows it down.
fn raw_roots(paths: &Paths) -> [PathBuf; 5] {
    [
        paths.claude_json.clone(),
        paths.skills_dir(),
        paths.agents_dir(),
        paths.plugins_dir(),
        paths.settings_json(),
    ]
}

/// `notify` refuses to watch a path that doesn't exist yet (unlike raw FSEvents, which tolerates
/// it), so a root that isn't there yet is watched from its nearest existing ancestor instead: once
/// that ancestor exists, a recursive watch still reports the root's own later creation.
fn watch_roots(paths: &Paths) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = raw_roots(paths)
        .into_iter()
        .map(|root| existing_ancestor(&root))
        .collect();
    roots.sort();
    roots.dedup();
    roots
}

fn existing_ancestor(path: &Path) -> PathBuf {
    let mut candidate = path;
    loop {
        if candidate.exists() {
            return candidate.to_path_buf();
        }
        match candidate.parent() {
            Some(parent) => candidate = parent,
            None => return candidate.to_path_buf(),
        }
    }
}

struct Throttler {
    throttle: Duration,
    on_change: Mutex<Box<dyn Fn() + Send>>,
    state: Mutex<ThrottlerState>,
    stopped: Arc<AtomicBool>,
}

#[derive(Default)]
struct ThrottlerState {
    last_fired: Option<Instant>,
    pending: bool,
    /// Set only while the callback itself is running, not while its window is still elapsing. A
    /// change arriving during the wait is already covered by the callback that is about to run; a
    /// change arriving during the callback is not.
    running: bool,
    retrigger: bool,
}

impl Throttler {
    fn new(throttle: Duration, on_change: Box<dyn Fn() + Send>, stopped: Arc<AtomicBool>) -> Self {
        Self {
            throttle,
            on_change: Mutex::new(on_change),
            state: Mutex::new(ThrottlerState::default()),
            stopped,
        }
    }

    /// Trailing-edge: the first change since idle fires right away (nothing to trail behind yet);
    /// anything arriving before the throttle window has elapsed since the last fire is coalesced
    /// into a single callback at the end of that window.
    fn trigger(self: &Arc<Self>) {
        let wait = {
            let mut state = self.lock_state();
            if state.pending {
                if state.running {
                    state.retrigger = true;
                }
                return;
            }
            let now = Instant::now();
            let elapsed = state
                .last_fired
                .map(|at| now.duration_since(at))
                .unwrap_or(self.throttle);
            state.pending = true;
            if elapsed < self.throttle {
                self.throttle - elapsed
            } else {
                Duration::ZERO
            }
        };

        let this = Arc::clone(self);
        thread::spawn(move || {
            if !wait.is_zero() {
                thread::sleep(wait);
            }
            // Checked immediately before the callback runs, not before the sleep: the watcher may
            // be dropped at any point during the wait, and a stale flag read at the start of the
            // wait would not catch a drop that happened after.
            if this.stopped.load(Ordering::SeqCst) {
                let mut state = this.lock_state();
                state.pending = false;
                state.retrigger = false;
                return;
            }
            this.lock_state().running = true;
            {
                let callback = this
                    .on_change
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                callback();
            }
            // Stamped after the callback returns, not before it starts: measuring the window from
            // when a refresh was picked up lets a refresh that outlasts the window queue one
            // thread per window, which then all run back to back the moment the first finishes.
            let again = {
                let mut state = this.lock_state();
                state.last_fired = Some(Instant::now());
                state.running = false;
                state.pending = false;
                std::mem::take(&mut state.retrigger)
            };
            if again {
                this.trigger();
            }
        });
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, ThrottlerState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_at(root: &Path) -> Paths {
        Paths {
            claude_dir: root.join(".claude"),
            claude_json: root.join(".claude.json"),
            support_dir: root.join("Support"),
            binary_override: None,
        }
    }

    #[test]
    fn is_of_interest_matches_the_watched_entry_itself() {
        let paths = paths_at(Path::new("/fake/home"));
        let watched = interesting_paths(&paths);
        assert!(is_of_interest(&paths.claude_json, &watched));
        assert!(is_of_interest(&paths.skills_dir(), &watched));
    }

    #[test]
    fn is_of_interest_matches_a_path_nested_under_a_watched_directory() {
        let paths = paths_at(Path::new("/fake/home"));
        let watched = interesting_paths(&paths);
        let nested = paths.skills_dir().join("my-skill").join("SKILL.md");
        assert!(is_of_interest(&nested, &watched));
    }

    #[test]
    fn is_of_interest_matches_an_ancestor_of_a_watched_file() {
        let paths = paths_at(Path::new("/fake/home"));
        let watched = interesting_paths(&paths);
        assert!(is_of_interest(&paths.plugins_dir(), &watched));
    }

    #[test]
    fn is_of_interest_matches_the_settings_file_and_registers_it_as_a_root() {
        let paths = paths_at(Path::new("/fake/home"));
        let watched = interesting_paths(&paths);
        assert!(is_of_interest(&paths.settings_json(), &watched));
        assert!(raw_roots(&paths).contains(&paths.settings_json()));
    }

    #[test]
    fn is_of_interest_rejects_an_unrelated_path() {
        let paths = paths_at(Path::new("/fake/home"));
        let watched = interesting_paths(&paths);
        assert!(!is_of_interest(
            Path::new("/fake/home/.claude/projects/foo/session.jsonl"),
            &watched
        ));
    }

    #[test]
    fn existing_ancestor_returns_the_path_itself_when_it_exists() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        assert_eq!(existing_ancestor(dir.path()), dir.path());
    }

    #[test]
    fn existing_ancestor_walks_up_to_the_nearest_real_directory() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let missing = dir.path().join("does-not-exist").join("skills");
        assert_eq!(existing_ancestor(&missing), dir.path());
    }

    #[test]
    fn resolve_best_effort_reattaches_the_non_existent_tail() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let canonical_dir = std::fs::canonicalize(dir.path()).expect("canonicalize");
        let target = dir.path().join("does-not-exist-yet").join(".claude.json");
        assert_eq!(
            resolve_best_effort(&target),
            canonical_dir
                .join("does-not-exist-yet")
                .join(".claude.json")
        );
    }
}
