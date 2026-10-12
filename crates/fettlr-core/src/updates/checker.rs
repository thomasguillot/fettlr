use std::collections::{BTreeMap, VecDeque};
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::civil_time;
use crate::cli_records::{CoreError, MarketplaceRecord, PluginRecord};
use crate::mcp::McpServer;
use crate::models::{UNKNOWN_VERSION, UpdateState};
use crate::skills::{Provenance, Skill};

use super::git::GitProbing;
use super::package_spec::PackageSpec;
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
}

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
        }
    }

    /// Checks every item the app can badge, in one pass. Never fails as a whole: a failure on one
    /// item leaves that item `Unknown` and the rest checked.
    ///
    /// `plugins` should be the installed records; `available` should be the marketplace catalogue
    /// (every listed plugin, including ones already installed). `PluginRecord::is_available_only`
    /// entries in `available` seed the catalogue lookup, so the checker never matches an installed
    /// plugin against itself.
    pub fn check(
        &self,
        marketplaces: &[MarketplaceRecord],
        plugins: &[PluginRecord],
        available: &[PluginRecord],
        skills: &[Skill],
        mcp: &[McpServer],
    ) -> UpdateReport {
        let catalogue = build_catalogue(available);

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
            let catalogue = &catalogue;
            jobs.push(Job {
                target: Target::Plugin,
                id: plugin.id(),
                compute: Box::new(move || plugin_state(plugin, catalogue)),
            });
        }

        for skill in skills {
            let git = self.git.as_ref();
            jobs.push(Job {
                target: Target::Skill,
                id: skill.id.clone(),
                compute: Box::new(move || skill_state(skill, git)),
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

fn skill_state(skill: &Skill, git: &dyn GitProbing) -> UpdateState {
    if skill.provenance != Provenance::GitClone {
        return UpdateState::Unknown;
    }
    behind_state(git.behind_count(&skill.path))
}

fn behind_state(result: Result<u32, CoreError>) -> UpdateState {
    match result {
        Ok(0) => UpdateState::UpToDate,
        Ok(commits) => UpdateState::Behind { commits },
        Err(_) => UpdateState::Unknown,
    }
}

fn build_catalogue(available: &[PluginRecord]) -> BTreeMap<String, PluginRecord> {
    let mut catalogue: BTreeMap<String, PluginRecord> = BTreeMap::new();
    for candidate in available.iter().filter(|plugin| plugin.is_available_only) {
        catalogue
            .entry(candidate.plugin_id.clone())
            .and_modify(|existing| {
                if is_newer(&candidate.version, &existing.version) {
                    *existing = candidate.clone();
                }
            })
            .or_insert_with(|| candidate.clone());
    }
    catalogue
}

fn plugin_state(plugin: &PluginRecord, catalogue: &BTreeMap<String, PluginRecord>) -> UpdateState {
    if plugin.is_available_only {
        return UpdateState::Unknown;
    }
    match catalogue.get(&plugin.plugin_id) {
        Some(latest) => version_state(&plugin.version, &latest.version),
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

fn is_newer(candidate: &str, existing: &str) -> bool {
    match (numeric_fields(candidate), numeric_fields(existing)) {
        (Some(candidate_fields), Some(existing_fields)) => {
            is_older(&existing_fields, &candidate_fields)
        }
        _ => false,
    }
}

/// A leading `v` is the one decoration common enough to be worth reading through: `v1.2.0` and
/// `1.2.0` are the same release, and treating them as uncomparable would report a permanent
/// unknown for a plugin that is simply up to date.
fn numeric_fields(version: &str) -> Option<Vec<i64>> {
    version
        .strip_prefix(['v', 'V'])
        .unwrap_or(version)
        .split('.')
        .map(|field| field.parse::<i64>().ok())
        .collect()
}

fn is_older(current: &[i64], newest: &[i64]) -> bool {
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
