use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, TryLockError};

use serde::Serialize;

use crate::cli::{ClaudeCli, CliRunner};
use crate::cli_records::{CoreError, McpConnector, McpServerConfig, merge_connectors};
use crate::discovery::{discover, ever_connected_connectors, plugin_skills, plugin_subagents};
use crate::json_store::{load_json_store, save_json_store};
use crate::mcp::{McpOrigin, McpServer, McpStore};
use crate::models::{Scope, UpdateState};
use crate::paths::Paths;
use crate::skills::{
    ParkKind, ParkedSkill, ParkedSubagent, Provenance, Skill, SkillStore, SkillsLock, SystemClock,
    is_plain_name, read_skills_lock, record_skill_update, recover_interrupted_swaps,
    replace_skill_folder, set_aside_path,
};
use crate::snapshot::Snapshot;
use crate::subagents::Subagent;
use crate::trashing::Trashing;
use crate::updates::{
    GitProbing, RegistryClient, UpdateChecker, checkable_source_url, read_plugin_catalogue,
    skill_source_repo,
};

#[derive(Clone, Debug, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorCache {
    checked_at: String,
    connectors: Vec<McpConnector>,
}

pub struct Fettlr {
    paths: Paths,
    cli: Arc<ClaudeCli>,
    git: Arc<dyn GitProbing>,
    trasher: Arc<dyn Trashing>,
    mcp_store: McpStore,
    skill_store: SkillStore,
    subagent_store: SkillStore,
    update_checker: UpdateChecker,
    snapshot: Mutex<Snapshot>,
    refresh_lock: Mutex<()>,
    skills_cli_update_lock: Mutex<()>,
}

impl Fettlr {
    pub fn new(
        paths: Paths,
        runner: Arc<dyn CliRunner>,
        git: Arc<dyn GitProbing>,
        registry: Arc<dyn RegistryClient>,
        trasher: Arc<dyn Trashing>,
    ) -> Self {
        let cli = Arc::new(ClaudeCli::new(runner, &paths));
        let mcp_store = McpStore::new(&paths, cli.clone());
        let skill_store = SkillStore::new(&paths, trasher.clone());
        let subagent_store = SkillStore::for_kind(
            ParkKind::Subagent,
            &paths,
            trasher.clone(),
            Arc::new(SystemClock),
        );
        let update_checker = UpdateChecker::new(git.clone(), registry)
            .with_source_cache(paths.skill_sources_cache());
        Self {
            paths,
            cli,
            git,
            trasher,
            mcp_store,
            skill_store,
            subagent_store,
            update_checker,
            snapshot: Mutex::new(Snapshot::default()),
            refresh_lock: Mutex::new(()),
            skills_cli_update_lock: Mutex::new(()),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.lock_snapshot().clone()
    }

    fn lock_snapshot(&self) -> std::sync::MutexGuard<'_, Snapshot> {
        self.snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Applies a change to the cached snapshot under the lock. Every caller here reads the lists it
    /// needs, spends seconds in `claude`, `git` or the registry, and only then has something to
    /// write. Writing a whole snapshot assembled from that earlier read would undo any mutation
    /// that landed meanwhile, so each writes back only the fields it actually computed.
    fn update(&self, edit: impl FnOnce(&mut Snapshot)) -> Snapshot {
        let mut snapshot = self.lock_snapshot();
        edit(&mut snapshot);
        snapshot.clone()
    }

    /// Discovery plus every CLI-derived list, assembled the way `AppModel.performRefresh()` does:
    /// a missing `claude` binary degrades to file-derived data with `cli_available` false rather
    /// than failing the whole refresh. `skills` is user skills, project skills, and every installed
    /// plugin's own skills merged into one list, matching `skills = config.userSkills +
    /// config.projectSkills + fromPlugins.skills` in Swift. `available_plugins` and `update_report`
    /// are carried over from the last `check_updates()`, since a plain refresh never touches either.
    pub fn refresh(&self) -> Result<Snapshot, CoreError> {
        let _ordered = self
            .refresh_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let updating = match self.skills_cli_update_lock.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        };
        if updating.is_some() {
            recover_interrupted_swaps(&self.paths);
        }
        drop(updating);
        let discovered = discover(&self.paths)?;
        let mut warnings = discovered.warnings.clone();
        let mut binary_missing = false;

        let plugins = match self.cli.list_plugins() {
            Ok(list) => list,
            Err(CoreError::BinaryNotFound) => {
                binary_missing = true;
                Vec::new()
            }
            Err(error) => {
                warnings.push(error.to_string());
                Vec::new()
            }
        };

        let marketplaces = match self.cli.list_marketplaces() {
            Ok(list) => list,
            Err(CoreError::BinaryNotFound) => {
                binary_missing = true;
                Vec::new()
            }
            Err(error) => {
                warnings.push(error.to_string());
                Vec::new()
            }
        };

        let mcp_servers = match self.mcp_store.assemble(&discovered, &plugins) {
            Ok(servers) => servers,
            Err(error) => {
                warnings.push(error.to_string());
                Vec::new()
            }
        };

        let parked_skills = match self.skill_store.parked() {
            Ok(records) => records,
            Err(error) => {
                warnings.push(error.to_string());
                Vec::new()
            }
        };

        let mut skills = discovered.user_skills;
        skills.extend(discovered.project_skills);
        let (from_plugins, plugin_skill_warnings) = plugin_skills(&plugins);
        skills.extend(from_plugins);
        warnings.extend(plugin_skill_warnings);

        let parked_subagents = match self.subagent_store.parked() {
            Ok(records) => records,
            Err(error) => {
                warnings.push(error.to_string());
                Vec::new()
            }
        };

        let mut subagents = discovered.user_subagents;
        subagents.extend(discovered.project_subagents);
        let (from_plugins, plugin_subagent_warnings) = plugin_subagents(&plugins);
        subagents.extend(from_plugins);
        warnings.extend(plugin_subagent_warnings);

        // available_plugins and update_report are left untouched: a plain refresh computes neither,
        // so they stay whatever the last check_updates() put there.
        Ok(self.update(|snapshot| {
            snapshot.cli_available = !binary_missing;
            snapshot.skills = skills;
            snapshot.parked_skills = parked_skills;
            snapshot.subagents = subagents;
            snapshot.parked_subagents = parked_subagents;
            snapshot.plugins = plugins;
            snapshot.marketplaces = marketplaces;
            snapshot.mcp_servers = mcp_servers;
            snapshot.projects = discovered.projects;
            snapshot.usage = discovered.usage;
            snapshot.warnings = warnings;
            snapshot.update_failures = Vec::new();
            snapshot.last_refreshed = Some(now_rfc3339());
            if snapshot.connectors_checked_at.is_none() {
                let cached = self.cached_connectors();
                if !cached.checked_at.is_empty() {
                    snapshot.connectors = cached.connectors;
                    snapshot.connectors_checked_at = Some(cached.checked_at);
                }
            }
        }))
    }

    /// Loads the plugin catalogue and checks every list the last refresh produced. Never re-runs
    /// discovery or the CLI list calls `refresh()` already did; a caller that also wants the very
    /// latest lists calls `refresh()` first.
    pub fn check_updates(&self) -> Result<Snapshot, CoreError> {
        let snapshot = self.snapshot();
        if !snapshot.cli_available {
            return Ok(snapshot);
        }

        let mut binary_missing = false;
        let mut warning = None;
        let available_plugins = match self.cli.plugin_catalogue() {
            Ok(list) => list,
            Err(CoreError::BinaryNotFound) => {
                binary_missing = true;
                Vec::new()
            }
            Err(error) => {
                warning = Some(error.to_string());
                Vec::new()
            }
        };

        let catalogue = read_plugin_catalogue(&self.paths);
        let report = self.update_checker.check(
            &snapshot.marketplaces,
            &snapshot.plugins,
            &catalogue,
            &snapshot.skills,
            &snapshot.mcp_servers,
        );

        Ok(self.update(|snapshot| {
            snapshot.available_plugins = available_plugins;
            snapshot.update_report = Some(report);
            if binary_missing {
                snapshot.cli_available = false;
            }
            if let Some(message) = warning
                && !snapshot.warnings.contains(&message)
            {
                snapshot.warnings.push(message);
            }
        }))
    }

    /// Reads the connectors Claude Code holds on the account. This is the one call that health
    /// checks every server on the machine, network and stdio alike, so it is never on a refresh
    /// and never on a timer: it runs when a reader asks for it, and the answer is cached.
    pub fn refresh_connectors(&self) -> Result<Snapshot, CoreError> {
        let listed = self.cli.mcp_list()?;
        let connectors = merge_connectors(&listed, &ever_connected_connectors(&self.paths));
        let checked_at = now_rfc3339();
        let cache = ConnectorCache {
            checked_at: checked_at.clone(),
            connectors: connectors.clone(),
        };
        save_json_store(&self.connectors_path(), &cache)?;
        Ok(self.update(|snapshot| {
            snapshot.connectors = connectors;
            snapshot.connectors_checked_at = Some(checked_at);
        }))
    }

    fn connectors_path(&self) -> PathBuf {
        self.paths.support_dir.join("connectors.json")
    }

    fn cached_connectors(&self) -> ConnectorCache {
        load_json_store(&self.connectors_path()).unwrap_or_default()
    }

    fn mutate(&self, body: impl FnOnce() -> Result<(), CoreError>) -> Result<Snapshot, CoreError> {
        let outcome = body();
        let refreshed = self.refresh();
        outcome?;
        refreshed
    }

    pub fn park_skill(&self, skill: &Skill) -> Result<Snapshot, CoreError> {
        self.mutate(|| {
            plugin_owned(skill)?;
            self.skill_store.park(skill).map(|_| ())
        })
    }

    pub fn restore_skill(&self, parked: &ParkedSkill) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.skill_store.restore(parked))
    }

    pub fn delete_skill(&self, skill: &Skill) -> Result<Snapshot, CoreError> {
        self.mutate(|| {
            plugin_owned(skill)?;
            self.skill_store.delete(skill)
        })
    }

    pub fn delete_parked_skill(&self, parked: &ParkedSkill) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.skill_store.delete_parked(parked))
    }

    pub fn park_subagent(&self, subagent: &Subagent) -> Result<Snapshot, CoreError> {
        self.mutate(|| {
            plugin_owned_subagent(subagent)?;
            self.subagent_store
                .park_path(
                    &subagent.name,
                    &subagent.scope,
                    &subagent.path,
                    &subagent.relative_path,
                )
                .map(|_| ())
        })
    }

    pub fn restore_subagent(&self, parked: &ParkedSubagent) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.subagent_store.restore(parked))
    }

    pub fn delete_subagent(&self, subagent: &Subagent) -> Result<Snapshot, CoreError> {
        self.mutate(|| {
            plugin_owned_subagent(subagent)?;
            self.subagent_store.delete_path(&subagent.path)
        })
    }

    pub fn delete_parked_subagent(&self, parked: &ParkedSubagent) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.subagent_store.delete_parked(parked))
    }

    /// Updates one skill from where it came from. A git clone is pulled fast-forward only, matching
    /// `GitProbing::pull_ff_only`, so a folder carrying local commits is left alone rather than
    /// merged. A skill installed by the skills CLI has its folder replaced from its source and the
    /// new folder hash recorded in the CLI's lock.
    pub fn pull_skill(&self, skill: &Skill) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.update_skill(skill))
    }

    fn update_skill(&self, skill: &Skill) -> Result<(), CoreError> {
        match &skill.provenance {
            Provenance::GitClone => self.git.pull_ff_only(&skill.path),
            Provenance::SkillsCli(_) => self.update_skills_cli_skill(&skill.name),
            _ => Err(CoreError::NotAGitClone {
                name: skill.name.clone(),
            }),
        }
    }

    /// Works from the lock as it reads now, not from the origin the caller holds, and on the
    /// literal canonical folder in the skills CLI's own directory, never on the link Claude Code
    /// reads through.
    fn update_skills_cli_skill(&self, name: &str) -> Result<(), CoreError> {
        let _serialized = self
            .skills_cli_update_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let unsupported = || CoreError::UnsupportedSkillSource {
            name: name.to_string(),
        };
        if !is_plain_name(name) {
            return Err(unsupported());
        }

        let target = self.paths.skills_cli_skills_dir().join(name);
        let metadata = std::fs::symlink_metadata(&target).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CoreError::MissingSkill {
                    path: target.display().to_string(),
                }
            } else {
                CoreError::Io(error)
            }
        })?;
        if !metadata.is_dir() {
            return Err(unsupported());
        }

        let lock = self.paths.skills_cli_lock();
        let SkillsLock::Read(entries) = read_skills_lock(&lock)? else {
            return Err(unsupported());
        };
        let origin = entries
            .get(name)
            .ok_or_else(|| CoreError::SkillsLockChanged {
                path: lock.display().to_string(),
            })?;
        let url = checkable_source_url(origin).ok_or_else(unsupported)?;
        let cache_repo =
            skill_source_repo(&self.paths.skill_sources_cache(), origin).ok_or_else(unsupported)?;

        let commit = self
            .git
            .fetch_source(url, origin.reference.as_deref(), &cache_repo)?;
        let tree = self
            .git
            .tree_sha(&cache_repo, &commit, &origin.folder)?
            .ok_or_else(|| CoreError::SkillGoneUpstream {
                name: name.to_string(),
                source_name: origin.source.clone(),
            })?;

        if tree.eq_ignore_ascii_case(&origin.folder_hash) {
            return Ok(());
        }
        self.swap_in_source_folder(name, &cache_repo, &commit, &origin.folder, &target)?;
        record_skill_update(&lock, name, &tree, &now_rfc3339())
    }

    fn swap_in_source_folder(
        &self,
        name: &str,
        cache_repo: &Path,
        commit: &str,
        folder: &str,
        target: &Path,
    ) -> Result<(), CoreError> {
        let cli_dir = self.paths.skills_cli_dir();
        let staging = cli_dir.join(".fettlr-staging");
        remove_if_present(&staging)?;
        std::fs::create_dir_all(&staging)?;

        let swapped = (|| {
            self.git
                .export_folder(cache_repo, commit, folder, &staging)?;
            let staged = if folder.is_empty() {
                staging.clone()
            } else {
                staging.join(folder)
            };
            let set_aside = set_aside_path(&self.paths, name);
            if let Some(parent) = set_aside.parent()
                && std::fs::symlink_metadata(parent).is_ok_and(|metadata| !metadata.is_dir())
            {
                return Err(CoreError::OccupiedDestination {
                    path: parent.display().to_string(),
                });
            }
            if std::fs::symlink_metadata(&set_aside).is_ok() {
                self.trasher.trash(&set_aside)?;
            }
            replace_skill_folder(&staged, target, &set_aside, self.trasher.as_ref())
        })();

        if let Err(error) = remove_if_present(&staging) {
            log::warn!("could not clear {}: {error}", staging.display());
        }
        swapped
    }

    /// The CLI's own description of one installed plugin.
    pub fn plugin_details(&self, id: &str) -> Result<String, CoreError> {
        self.cli.plugin_details(id)
    }

    /// The CLI's own description of one MCP server.
    pub fn mcp_get(&self, name: &str, project_path: Option<&str>) -> Result<String, CoreError> {
        self.cli.mcp_get(name, project_path)
    }

    pub fn install_plugin(&self, plugin_id: &str, scope: &Scope) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.cli.install_plugin(plugin_id, scope))
    }

    pub fn uninstall_plugin(&self, plugin_id: &str, scope: &Scope) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.cli.uninstall_plugin(plugin_id, scope))
    }

    pub fn set_plugin_enabled(
        &self,
        plugin_id: &str,
        enabled: bool,
        scope: &Scope,
    ) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.cli.set_plugin_enabled(plugin_id, enabled, scope))
    }

    pub fn update_plugin(&self, plugin_id: &str, scope: &Scope) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.cli.update_plugin(plugin_id, scope))
    }

    pub fn add_marketplace(&self, source: &str) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.cli.add_marketplace(source))
    }

    pub fn remove_marketplace(&self, name: &str) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.cli.remove_marketplace(name))
    }

    /// `name: None` updates every marketplace in one CLI call.
    pub fn update_marketplace(&self, name: Option<&str>) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.cli.update_marketplace(name))
    }

    pub fn add_mcp_server(
        &self,
        name: &str,
        config: &McpServerConfig,
        origin: &McpOrigin,
    ) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.mcp_store.add(name, config, origin))
    }

    pub fn edit_mcp_server(
        &self,
        server: &McpServer,
        new_name: &str,
        new_config: &McpServerConfig,
    ) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.mcp_store.edit(server, new_name, new_config))
    }

    pub fn remove_mcp_server(&self, server: &McpServer) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.mcp_store.remove(server))
    }

    pub fn promote_mcp_server(&self, server: &McpServer) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.mcp_store.promote_to_user(server))
    }

    pub fn set_mcp_server_enabled(
        &self,
        server: &McpServer,
        enabled: bool,
    ) -> Result<Snapshot, CoreError> {
        self.mutate(|| self.mcp_store.set_enabled(server, enabled))
    }

    /// Marketplaces, then plugins, then updatable skills, in that order and one at a time. MCP
    /// servers are never included: Claude Code has no update verb for them, only a config repin
    /// (`edit_mcp_server`). A failed item is recorded in the returned snapshot's `update_failures`
    /// and the run continues; the state is re-read once at the end, not after every item.
    pub fn update_all(&self, progress: &dyn Fn(UpdateProgress)) -> Result<Snapshot, CoreError> {
        let cached = self.snapshot();
        let items = stale_items(&cached);
        let mut failures = Vec::new();

        for item in &items {
            progress(UpdateProgress {
                kind: item.kind(),
                id: item.id().to_string(),
                stage: UpdateStage::Running,
            });
            let outcome = match item {
                StaleItem::Marketplace { name } => self.cli.update_marketplace(Some(name)),
                StaleItem::Plugin {
                    plugin_id, scope, ..
                } => self.cli.update_plugin(plugin_id, scope),
                StaleItem::Skill { skill, .. } => self.update_skill(skill),
            };
            let stage = if outcome.is_ok() {
                UpdateStage::Updated
            } else {
                UpdateStage::Failed
            };
            progress(UpdateProgress {
                kind: item.kind(),
                id: item.id().to_string(),
                stage,
            });
            if let Err(error) = outcome {
                failures.push(UpdateFailure {
                    kind: item.kind(),
                    name: item.display_name().to_string(),
                    error: error.to_string(),
                });
            }
        }

        self.refresh()?;
        Ok(self.update(|snapshot| {
            snapshot.update_failures = failures;
        }))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateProgress {
    pub kind: UpdateItemKind,
    pub id: String,
    pub stage: UpdateStage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum UpdateStage {
    Running,
    Updated,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum UpdateItemKind {
    Marketplace,
    Plugin,
    Skill,
}

/// One item `update_all` could not update, kept apart from the message so the window words it in
/// the reader's own language rather than matching on English text the core happens to produce.
#[derive(Clone, Debug, Serialize)]
pub struct UpdateFailure {
    pub kind: UpdateItemKind,
    pub name: String,
    pub error: String,
}

enum StaleItem {
    Marketplace {
        name: String,
    },
    Plugin {
        id: String,
        plugin_id: String,
        scope: Scope,
        display_name: String,
    },
    Skill {
        id: String,
        skill: Box<Skill>,
    },
}

// A plugin's skill folder belongs to the plugin's own install, so moving or trashing it corrupts
// the plugin. The window never offers either action for one; this refuses it at the core anyway.
fn plugin_owned(skill: &Skill) -> Result<(), CoreError> {
    if let Provenance::Plugin { plugin_id } = &skill.provenance {
        return Err(CoreError::SkillManagedByPlugin {
            name: skill.name.clone(),
            plugin_id: plugin_id.clone(),
        });
    }
    Ok(())
}

fn plugin_owned_subagent(subagent: &Subagent) -> Result<(), CoreError> {
    if let Provenance::Plugin { plugin_id } = &subagent.provenance {
        return Err(CoreError::SubagentManagedByPlugin {
            name: subagent.name.clone(),
            plugin_id: plugin_id.clone(),
        });
    }
    Ok(())
}

impl StaleItem {
    fn display_name(&self) -> &str {
        match self {
            StaleItem::Marketplace { name } => name,
            StaleItem::Plugin { display_name, .. } => display_name,
            StaleItem::Skill { skill, .. } => &skill.name,
        }
    }

    fn id(&self) -> &str {
        match self {
            StaleItem::Marketplace { name } => name,
            StaleItem::Plugin { id, .. } | StaleItem::Skill { id, .. } => id,
        }
    }

    fn kind(&self) -> UpdateItemKind {
        match self {
            StaleItem::Marketplace { .. } => UpdateItemKind::Marketplace,
            StaleItem::Plugin { .. } => UpdateItemKind::Plugin,
            StaleItem::Skill { .. } => UpdateItemKind::Skill,
        }
    }
}

/// Everything the last update check called stale, in the order `update_all` works through it.
/// Mirrors the Swift app's `UpdateItem.stale(...)` filtered by `isUpdatable` (marketplaces and
/// plugins always qualify, a skill only if it is a git clone or came from the skills CLI, MCP
/// servers never).
fn stale_items(snapshot: &Snapshot) -> Vec<StaleItem> {
    let mut items = Vec::new();
    let Some(report) = &snapshot.update_report else {
        return items;
    };

    for marketplace in &snapshot.marketplaces {
        if report
            .marketplaces
            .get(&marketplace.name)
            .is_some_and(UpdateState::is_stale)
        {
            items.push(StaleItem::Marketplace {
                name: marketplace.name.clone(),
            });
        }
    }

    for plugin in &snapshot.plugins {
        if !plugin.is_available_only
            && plugin.scope.cli_value().is_some()
            && report
                .plugins
                .get(&plugin.id())
                .is_some_and(UpdateState::is_stale)
        {
            items.push(StaleItem::Plugin {
                id: plugin.id(),
                plugin_id: plugin.plugin_id.clone(),
                scope: plugin.scope.clone(),
                display_name: plugin.name.clone(),
            });
        }
    }

    for skill in &snapshot.skills {
        if matches!(
            skill.provenance,
            Provenance::GitClone | Provenance::SkillsCli(_)
        ) && report
            .skills
            .get(&skill.id)
            .is_some_and(UpdateState::is_stale)
        {
            items.push(StaleItem::Skill {
                id: skill.id.clone(),
                skill: Box::new(skill.clone()),
            });
        }
    }

    items
}

fn remove_if_present(path: &Path) -> Result<(), CoreError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(std::fs::remove_dir_all(path)?),
        Ok(_) => Ok(std::fs::remove_file(path)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn now_rfc3339() -> String {
    crate::civil_time::format_rfc3339(crate::civil_time::now_unix_seconds())
}
