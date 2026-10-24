use std::collections::{BTreeMap, VecDeque};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::civil_time;
use crate::cli_records::{CoreError, MarketplaceRecord, PluginRecord};
use crate::mcp::McpServer;
use crate::models::{UNKNOWN_VERSION, UpdateState};
use crate::skills::{Provenance, Skill, SkillsCliOrigin};

use super::git::GitProbing;
use super::package_spec::PackageSpec;
use super::plugin_catalogue::{CatalogueVersion, PluginCatalogue};
use super::registry::{PackageRegistry, RegistryClient};

#[derive(Clone, Debug, Default, Serialize)]
pub struct UpdateReport {
    pub marketplaces: BTreeMap<String, UpdateState>,
    pub plugins: BTreeMap<String, UpdateState>,
    pub skills: BTreeMap<String, UpdateState>,
    pub mcp_servers: BTreeMap<String, UpdateState>,
    pub checked_at: String,
}

impl UpdateReport {
    /// Every id this report calls stale, per list, so that a caller badges and lists exactly what
    /// `update_all` will work through instead of deciding for itself. MCP servers are included
    /// because the window offers a repin for them, even though `update_all` never touches one.
    pub fn stale_ids(&self) -> StaleIds {
        StaleIds {
            marketplaces: stale_keys(&self.marketplaces),
            plugins: stale_keys(&self.plugins),
            skills: stale_keys(&self.skills),
            mcp_servers: stale_keys(&self.mcp_servers),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StaleIds {
    pub marketplaces: Vec<String>,
    pub plugins: Vec<String>,
    pub skills: Vec<String>,
    pub mcp_servers: Vec<String>,
}

fn stale_keys(states: &BTreeMap<String, UpdateState>) -> Vec<String> {
    states
        .iter()
        .filter(|(_, state)| state.is_stale())
        .map(|(id, _)| id.clone())
        .collect()
}

pub struct UpdateChecker {
    git: Arc<dyn GitProbing>,
    registry: Arc<dyn RegistryClient>,
    registry_timeout: Duration,
    source_cache: Option<PathBuf>,
}

type SourceHeads = Mutex<BTreeMap<PathBuf, Arc<OnceLock<Option<String>>>>>;

impl UpdateChecker {
    const MAX_IN_FLIGHT: usize = 6;
    const DEFAULT_REGISTRY_TIMEOUT: Duration = Duration::from_secs(10);

    pub fn new(git: Arc<dyn GitProbing>, registry: Arc<dyn RegistryClient>) -> Self {
        Self::with_registry_timeout(git, registry, Self::DEFAULT_REGISTRY_TIMEOUT)
    }

    pub fn with_registry_timeout(
        git: Arc<dyn GitProbing>,
        registry: Arc<dyn RegistryClient>,
        registry_timeout: Duration,
    ) -> Self {
        Self {
            git,
            registry,
            registry_timeout,
            source_cache: None,
        }
    }

    /// Lets skills installed by the skills CLI be checked, by fetching their sources into `dir`.
    /// Without it every such skill is `Unknown`.
    pub fn with_source_cache(mut self, dir: PathBuf) -> Self {
        self.source_cache = Some(dir);
        self
    }

    /// Checks every item the app can badge, in one pass. Never fails as a whole: a failure on one
    /// item leaves that item `Unknown` and the rest checked.
    ///
    /// `plugins` should be the installed records; `catalogue` the versions the marketplaces on disk
    /// currently offer, as read by `read_plugin_catalogue`.
    pub fn check(
        &self,
        marketplaces: &[MarketplaceRecord],
        plugins: &[PluginRecord],
        catalogue: &PluginCatalogue,
        skills: &[Skill],
        mcp: &[McpServer],
    ) -> UpdateReport {
        let heads: SourceHeads = Mutex::new(BTreeMap::new());
        let heads = &heads;
        let mut jobs: Vec<Job<'_>> = Vec::new();

        for record in marketplaces {
            let git = self.git.as_ref();
            jobs.push(Job {
                target: Target::Marketplace,
                id: record.name.clone(),
                compute: Box::new(move || marketplace_state(record, git)),
            });
        }

        for plugin in plugins {
            jobs.push(Job {
                target: Target::Plugin,
                id: plugin.id(),
                compute: Box::new(move || plugin_state(plugin, catalogue)),
            });
        }

        for skill in skills {
            let git = self.git.as_ref();
            let cache = self.source_cache.as_deref();
            jobs.push(Job {
                target: Target::Skill,
                id: skill.id.clone(),
                compute: Box::new(move || skill_state(skill, git, cache, heads)),
            });
        }

        for server in mcp {
            let registry = Arc::clone(&self.registry);
            let timeout = self.registry_timeout;
            jobs.push(Job {
                target: Target::McpServer,
                id: server.id.clone(),
                compute: Box::new(move || mcp_state(server, registry, timeout)),
            });
        }

        let mut report = UpdateReport::default();
        for outcome in run_jobs(jobs) {
            let bucket = match outcome.target {
                Target::Marketplace => &mut report.marketplaces,
                Target::Plugin => &mut report.plugins,
                Target::Skill => &mut report.skills,
                Target::McpServer => &mut report.mcp_servers,
            };
            bucket.insert(outcome.id, outcome.state);
        }
        report.checked_at = civil_time::format_rfc3339(civil_time::now_unix_seconds());
        report
    }
}

enum Target {
    Marketplace,
    Plugin,
    Skill,
    McpServer,
}

/// `id` is computed eagerly, before `compute` runs, so a panicking `compute` (an injected
/// `GitProbing`/`RegistryClient` misbehaving) still leaves an id to report `Unknown` under instead
/// of losing the item from the report entirely.
struct Job<'a> {
    target: Target,
    id: String,
    compute: Box<dyn FnOnce() -> UpdateState + Send + 'a>,
}

struct JobResult {
    target: Target,
    id: String,
    state: UpdateState,
}

fn run_jobs<'a>(jobs: Vec<Job<'a>>) -> Vec<JobResult> {
    if jobs.is_empty() {
        return Vec::new();
    }

    let worker_count = UpdateChecker::MAX_IN_FLIGHT.min(jobs.len());
    let queue: Mutex<VecDeque<Job<'a>>> = Mutex::new(VecDeque::from(jobs));
    let results = Mutex::new(Vec::new());

    thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let job = queue.lock().expect("lock poisoned").pop_front();
                    let Some(Job {
                        target,
                        id,
                        compute,
                    }) = job
                    else {
                        break;
                    };
                    let state = panic::catch_unwind(AssertUnwindSafe(compute))
                        .unwrap_or(UpdateState::Unknown);
                    results
                        .lock()
                        .expect("lock poisoned")
                        .push(JobResult { target, id, state });
                }
            });
        }
    });

    results.into_inner().expect("lock poisoned")
}

fn marketplace_state(record: &MarketplaceRecord, git: &dyn GitProbing) -> UpdateState {
    if record.install_location.is_empty() || !matches!(record.source.as_str(), "github" | "git") {
        return UpdateState::Unknown;
    }
    behind_state(git.behind_count(Path::new(&record.install_location)))
}

fn skill_state(
    skill: &Skill,
    git: &dyn GitProbing,
    source_cache: Option<&Path>,
    heads: &SourceHeads,
) -> UpdateState {
    match &skill.provenance {
        Provenance::GitClone => behind_state(git.behind_count(&skill.path)),
        Provenance::SkillsCli(origin) => match source_cache {
            Some(cache) => source_state(origin, git, cache, heads),
            None => UpdateState::Unknown,
        },
        _ => UpdateState::Unknown,
    }
}

fn is_tree_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn source_state(
    origin: &SkillsCliOrigin,
    git: &dyn GitProbing,
    cache_root: &Path,
    heads: &SourceHeads,
) -> UpdateState {
    let Some(url) = checkable_source_url(origin) else {
        return UpdateState::Unknown;
    };

    let Some(repo) = skill_source_repo(cache_root, origin) else {
        return UpdateState::Unknown;
    };
    let slot = Arc::clone(
        heads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(repo.clone())
            .or_default(),
    );
    let head = slot.get_or_init(|| {
        git.fetch_source(url, origin.reference.as_deref(), &repo)
            .ok()
    });
    let Some(commit) = head else {
        return UpdateState::Unknown;
    };

    match git.tree_sha(&repo, commit, &origin.folder) {
        Ok(Some(latest)) if latest.eq_ignore_ascii_case(&origin.folder_hash) => {
            UpdateState::UpToDate
        }
        Ok(Some(_)) => UpdateState::SourceChanged,
        Ok(None) | Err(_) => UpdateState::Unknown,
    }
}

/// The url to fetch for an origin Fettlr can both check and update, or `None` when it can do
/// neither. The check and the update both ask this, so they never disagree about a skill.
pub(crate) fn checkable_source_url(origin: &SkillsCliOrigin) -> Option<&str> {
    let url = origin
        .source_url
        .as_deref()
        .filter(|url| url.starts_with("https://"))?;
    let folder_stays_inside = Path::new(&origin.folder)
        .components()
        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    (origin.source_type == "github" && is_tree_sha(&origin.folder_hash) && folder_stays_inside)
        .then_some(url)
}

/// The cache repository for one `(source url, reference)` pair under `cache_root`, or `None` when
/// the source names no usable directory. The check and the update both go through this so they
/// share one fetch. The name is lowercased for case-insensitive volumes and carries a hash of the
/// raw url and reference so keys the sanitizer folds together stay apart.
pub(crate) fn skill_source_repo(cache_root: &Path, origin: &SkillsCliOrigin) -> Option<PathBuf> {
    let sanitize = |value: &str| -> String {
        value
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect()
    };
    let mut key = sanitize(&origin.source);
    if key.chars().all(|c| c == '.') {
        return None;
    }
    if let Some(reference) = &origin.reference {
        key.push('@');
        key.push_str(&sanitize(reference));
    }
    let mut raw = origin.source_url.clone().unwrap_or_default().into_bytes();
    raw.push(0);
    raw.extend_from_slice(origin.reference.as_deref().unwrap_or("").as_bytes());
    Some(cache_root.join(format!("{key}-{:08x}", fnv1a(&raw))))
}

fn fnv1a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    })
}

fn behind_state(result: Result<u32, CoreError>) -> UpdateState {
    match result {
        Ok(0) => UpdateState::UpToDate,
        Ok(commits) => UpdateState::Behind { commits },
        Err(_) => UpdateState::Unknown,
    }
}

fn plugin_state(plugin: &PluginRecord, catalogue: &PluginCatalogue) -> UpdateState {
    if plugin.is_available_only {
        return UpdateState::Unknown;
    }
    match catalogue.get(&plugin.plugin_id) {
        Some(CatalogueVersion::Version(latest)) => version_state(&plugin.version, latest),
        Some(CatalogueVersion::Commit(sha)) => commit_state(&plugin.version, sha),
        Some(CatalogueVersion::Untracked) | None => UpdateState::Unknown,
    }
}

fn commit_state(installed: &str, sha: &str) -> UpdateState {
    let is_short_sha = installed.len() == 12
        && installed
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    if !is_short_sha {
        return UpdateState::Unknown;
    }
    let sha = sha.to_ascii_lowercase();
    if sha.starts_with(installed) {
        return UpdateState::UpToDate;
    }
    match sha.get(..12) {
        Some(latest) => UpdateState::UpdateAvailable {
            installed: installed.to_string(),
            latest: latest.to_string(),
        },
        None => UpdateState::Unknown,
    }
}

fn version_state(installed: &str, latest: &str) -> UpdateState {
    if installed == latest {
        return UpdateState::UpToDate;
    }
    match (numeric_fields(installed), numeric_fields(latest)) {
        (Some(current), Some(newest)) if is_older(&current, &newest) => {
            UpdateState::UpdateAvailable {
                installed: installed.to_string(),
                latest: latest.to_string(),
            }
        }
        (Some(_), Some(_)) => UpdateState::UpToDate,
        // The "unknown" sentinel is not an uncomparable version: it stays an available update so
        // the pane can name what the other side holds. `is_stale` drops only a sentinel `latest`.
        _ if installed == UNKNOWN_VERSION || latest == UNKNOWN_VERSION => {
            UpdateState::UpdateAvailable {
                installed: installed.to_string(),
                latest: latest.to_string(),
            }
        }
        // Two real versions we cannot order say nothing about which is newer. Claiming an update
        // offers one that never clears: the version is still uncomparable after it is applied, so
        // every check asks for it again and every Update All reports success over nothing.
        _ => UpdateState::Unknown,
    }
}

/// A leading `v` is the one decoration common enough to be worth reading through: `v1.2.0` and
/// `1.2.0` are the same release, and treating them as uncomparable would report a permanent
/// unknown for a plugin that is simply up to date.
pub(crate) fn numeric_fields(version: &str) -> Option<Vec<i64>> {
    version
        .strip_prefix(['v', 'V'])
        .unwrap_or(version)
        .split('.')
        .map(|field| field.parse::<i64>().ok())
        .collect()
}

pub(crate) fn is_older(current: &[i64], newest: &[i64]) -> bool {
    for index in 0..current.len().max(newest.len()) {
        let left = current.get(index).copied().unwrap_or(0);
        let right = newest.get(index).copied().unwrap_or(0);
        if left != right {
            return left < right;
        }
    }
    false
}

fn mcp_state(
    server: &McpServer,
    registry: Arc<dyn RegistryClient>,
    timeout: Duration,
) -> UpdateState {
    let Some(spec) = PackageSpec::parse(
        server.config.command.as_deref(),
        server.config.args.as_deref(),
    ) else {
        return UpdateState::Unknown;
    };
    let Some(pinned) = spec.pinned_version else {
        return UpdateState::TracksLatest;
    };

    match registry_lookup_with_timeout(registry, spec.registry, spec.name, timeout) {
        Ok(latest) => version_state(&pinned, &latest),
        Err(_) => UpdateState::Unknown,
    }
}

fn registry_lookup_with_timeout(
    registry: Arc<dyn RegistryClient>,
    package_registry: PackageRegistry,
    package: String,
    timeout: Duration,
) -> Result<String, CoreError> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(registry.latest_version(package_registry, &package));
    });
    receiver.recv_timeout(timeout).unwrap_or_else(|_| {
        Err(CoreError::Timeout {
            command: "registry lookup".to_string(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(source: &str, url: &str, reference: Option<&str>) -> SkillsCliOrigin {
        SkillsCliOrigin {
            source: source.to_string(),
            source_type: "github".to_string(),
            source_url: Some(url.to_string()),
            reference: reference.map(str::to_string),
            folder: "skills/a".to_string(),
            folder_hash: "0".repeat(40),
        }
    }

    #[test]
    fn a_source_key_of_only_dots_or_empty_has_no_cache_repo() {
        let root = Path::new("/cache/skill-sources");
        for source in ["", ".", ".."] {
            assert_eq!(
                skill_source_repo(root, &origin(source, "https://x/y.git", None)),
                None
            );
        }
        let inside = skill_source_repo(root, &origin("../x", "https://x/y.git", None))
            .expect("a repo for ../x");
        assert!(inside.starts_with(root));
    }

    #[test]
    fn the_cache_repo_name_is_stable_and_separates_url_and_reference() {
        let root = Path::new("/cache/skill-sources");
        let url = "https://github.com/emilkowalski/skills.git";
        assert_eq!(
            skill_source_repo(root, &origin("emilkowalski/skills", url, None)),
            Some(root.join("emilkowalski_skills-64c61f24"))
        );

        let slash = skill_source_repo(root, &origin("o/r", url, Some("feature/x"))).unwrap();
        let underscore = skill_source_repo(root, &origin("o/r", url, Some("feature_x"))).unwrap();
        assert_ne!(slash, underscore);

        let upper = skill_source_repo(
            root,
            &origin("Owner/skills", "https://github.com/Owner/skills.git", None),
        )
        .unwrap();
        let lower = skill_source_repo(
            root,
            &origin("owner/skills", "https://github.com/owner/skills.git", None),
        )
        .unwrap();
        assert_ne!(upper, lower);

        for path in [&slash, &underscore, &upper, &lower] {
            assert!(path.starts_with(root));
        }
    }
}
