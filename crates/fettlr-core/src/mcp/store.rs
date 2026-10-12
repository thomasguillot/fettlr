use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::cli_records::{CoreError, McpServerConfig, PluginRecord};
use crate::discovery::DiscoveredConfig;
use crate::json_store::{load_json_store, save_json_store};
use crate::models::Scope;
use crate::paths::Paths;
use crate::skills::{Clock, SystemClock};

use super::claude_json::{
    forget_project_local_server, forget_project_mcpjson_server, set_project_mcp_server,
    set_project_mcpjson_server,
};
use super::{McpMutating, McpOrigin, McpServer};

pub trait MonotonicClock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Debug, Default)]
pub struct SystemMonotonicClock;

impl MonotonicClock for SystemMonotonicClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DisabledMcpServer {
    id: String,
    name: String,
    origin: McpOrigin,
    config: McpServerConfig,
    disabled_at: String,
}

impl DisabledMcpServer {
    fn new(name: &str, origin: McpOrigin, config: McpServerConfig, disabled_at: String) -> Self {
        Self {
            id: format!("{}/{name}", origin.key()),
            name: name.to_string(),
            origin,
            config,
            disabled_at,
        }
    }
}

#[derive(Default)]
struct StoreState {
    assembled: Vec<McpServer>,
    pending_mutations: usize,
    last_mutation_at: Option<Instant>,
}

pub struct McpStore {
    paths: Paths,
    cli: Arc<dyn McpMutating>,
    clock: Arc<dyn Clock>,
    monotonic: Arc<dyn MonotonicClock>,
    state: Mutex<StoreState>,
    /// Serialises every read-modify-write of `mcp-disabled.json`, which two parks or a park and an
    /// assemble would otherwise interleave and lose a record between. Never held across a CLI call:
    /// the refresh Claude Code's own write sets off re-enters `assemble`, which takes it too.
    records: Mutex<()>,
}

impl McpStore {
    pub fn new(paths: &Paths, cli: Arc<dyn McpMutating>) -> Self {
        Self::with_clocks(
            paths,
            cli,
            Arc::new(SystemClock),
            Arc::new(SystemMonotonicClock),
        )
    }

    pub fn with_clocks(
        paths: &Paths,
        cli: Arc<dyn McpMutating>,
        clock: Arc<dyn Clock>,
        monotonic: Arc<dyn MonotonicClock>,
    ) -> Self {
        Self {
            paths: paths.clone(),
            cli,
            clock,
            monotonic,
            state: Mutex::new(StoreState::default()),
            records: Mutex::new(()),
        }
    }

    pub fn assemble(
        &self,
        config: &DiscoveredConfig,
        plugins: &[PluginRecord],
    ) -> Result<Vec<McpServer>, CoreError> {
        let mut servers = Vec::new();

        for (name, server_config) in &config.user_mcp_servers {
            servers.push(McpServer::new(
                name,
                McpOrigin::UserScope,
                server_config.clone(),
                true,
            ));
        }

        for project in &config.projects {
            let disabled_local: HashSet<&str> = project
                .disabled_mcp_servers
                .iter()
                .map(String::as_str)
                .collect();
            for (name, server_config) in &project.local_mcp_servers {
                servers.push(McpServer::new(
                    name,
                    McpOrigin::LocalScope {
                        project: project.path.clone(),
                    },
                    server_config.clone(),
                    !disabled_local.contains(name.as_str()),
                ));
            }
            let disabled: HashSet<&str> = project
                .disabled_mcpjson_servers
                .iter()
                .map(String::as_str)
                .collect();
            let Some(entries) = config.project_mcp_json.get(&project.path) else {
                continue;
            };
            for (name, server_config) in entries {
                servers.push(McpServer::new(
                    name,
                    McpOrigin::ProjectFile {
                        project: project.path.clone(),
                    },
                    server_config.clone(),
                    !disabled.contains(name.as_str()),
                ));
            }
        }

        let mut plugin_servers: HashMap<String, usize> = HashMap::new();
        for plugin in plugins {
            let Some(entries) = &plugin.mcp_servers else {
                continue;
            };
            for (name, server_config) in entries {
                let server = McpServer::new(
                    name,
                    McpOrigin::Plugin {
                        plugin_id: plugin.plugin_id.clone(),
                    },
                    server_config.clone(),
                    plugin.enabled,
                );
                match plugin_servers.get(&server.id) {
                    Some(&index) => {
                        servers[index].is_enabled = servers[index].is_enabled || plugin.enabled;
                    }
                    None => {
                        plugin_servers.insert(server.id.clone(), servers.len());
                        servers.push(server);
                    }
                }
            }
        }

        // A record whose server is live again is stale, but only a config read taken after
        // Fettlr's own last change may say so: `claude` rewrites `~/.claude.json` before applying
        // a mutation, so a refresh woken by that write still lists a server Fettlr just parked,
        // and dropping the record on that snapshot destroys the config the user asked it to keep.
        let live: HashSet<String> = servers.iter().map(|server| server.id.clone()).collect();
        {
            let _records = self.lock_records();
            let records = self.disabled_records()?;
            let trusted = {
                let state = self.state();
                state.pending_mutations == 0
                    && state
                        .last_mutation_at
                        .is_none_or(|mutated_at| config.read_at > mutated_at)
            };
            let kept: Vec<DisabledMcpServer> = if trusted {
                records
                    .iter()
                    .filter(|record| !live.contains(&record.id))
                    .cloned()
                    .collect()
            } else {
                records.clone()
            };
            for record in kept.iter().filter(|record| !live.contains(&record.id)) {
                servers.push(McpServer::new(
                    &record.name,
                    record.origin.clone(),
                    record.config.clone(),
                    false,
                ));
            }
            if kept.len() != records.len() {
                self.save_disabled_records(&kept)?;
            }
        }

        servers.sort_by_key(|server| (server.origin.key(), server.name.clone()));
        self.state().assembled = servers.clone();
        Ok(servers)
    }

    pub fn add(
        &self,
        name: &str,
        config: &McpServerConfig,
        origin: &McpOrigin,
    ) -> Result<(), CoreError> {
        let _guard = self.begin_mutation();
        let (scope, local) = origin.mutable_scope()?;
        if self
            .state()
            .assembled
            .iter()
            .any(|server| &server.origin == origin && server.name == name)
        {
            return Err(CoreError::DuplicateMcpName {
                name: name.to_string(),
            });
        }
        // Fettlr no longer creates an orphaned name, but Claude Code and older builds have, and
        // one left behind would land this server switched off the moment it appears.
        self.forget_project_name(origin, name);
        self.cli.mcp_add_json(name, &encode(config)?, &scope, local)
    }

    pub fn promote_to_user(&self, server: &McpServer) -> Result<(), CoreError> {
        let _guard = self.begin_mutation();
        if !matches!(server.origin, McpOrigin::LocalScope { .. }) {
            return Err(CoreError::NotPromotable {
                origin: server.origin.key(),
            });
        }
        // Promoting adds at user scope before removing at local scope. For a server that is
        // switched off there is nothing to remove, so the add would succeed on its own and leave
        // the server running at a wider scope than it was ever enabled at.
        if !server.is_enabled {
            return Err(CoreError::PromoteDisabled {
                name: server.name.clone(),
            });
        }
        if self.state().assembled.iter().any(|candidate| {
            candidate.origin == McpOrigin::UserScope && candidate.name == server.name
        }) {
            return Err(CoreError::DuplicateMcpName {
                name: server.name.clone(),
            });
        }

        let (scope, local) = server.origin.mutable_scope()?;
        self.cli
            .mcp_add_json(&server.name, &encode(&server.config)?, &Scope::User, false)?;
        // The add must land before the remove: a failed add leaves the server where it was, while a
        // remove issued first and then failing to add would lose the config outright. The reverse
        // failure, a successful add followed by a failed remove, only leaves the server in both
        // scopes, which is recoverable.
        self.cli.mcp_remove(&server.name, &scope, local)
    }

    pub fn edit(
        &self,
        server: &McpServer,
        new_name: &str,
        new_config: &McpServerConfig,
    ) -> Result<(), CoreError> {
        let _guard = self.begin_mutation();
        let (scope, local) = server.origin.mutable_scope()?;
        if self.state().assembled.iter().any(|candidate| {
            candidate.origin == server.origin
                && candidate.name == new_name
                && candidate.id != server.id
        }) {
            return Err(CoreError::DuplicateMcpName {
                name: new_name.to_string(),
            });
        }

        {
            let _records = self.lock_records();
            let mut records = self.disabled_records()?;
            if !server.is_enabled
                && let Some(index) = records.iter().position(|record| record.id == server.id)
            {
                records[index] = DisabledMcpServer::new(
                    new_name,
                    server.origin.clone(),
                    new_config.clone(),
                    records[index].disabled_at.clone(),
                );
                return self.save_disabled_records(&records);
            }
        }

        // Both encodings happen before the remove: an edit that fails afterwards has to have the
        // original JSON, secrets included, in hand to put the server back.
        let original_json = encode(&server.config)?;
        let updated_json = encode(new_config)?;

        // The new name is listed as disabled before it exists, not after. Absence from the array
        // means enabled, so doing this afterwards leaves a window, and a failure inside it, with a
        // server the reader had switched off now renamed and running.
        let renaming_disabled = !server.is_enabled && new_name != server.name;
        if renaming_disabled {
            self.set_project_disabled(&server.origin, new_name, false)?;
        }

        self.cli.mcp_remove(&server.name, &scope, local)?;
        if let Err(error) = self
            .cli
            .mcp_add_json(new_name, &updated_json, &scope, local)
        {
            if renaming_disabled {
                self.forget_project_name(&server.origin, new_name);
            }
            if self
                .cli
                .mcp_add_json(&server.name, &original_json, &scope, local)
                .is_err()
            {
                return Err(CoreError::UnrestorableMcpServer {
                    name: server.name.clone(),
                    config_json: original_json,
                });
            }
            return Err(error);
        }

        if new_name != server.name {
            self.forget_project_name(&server.origin, &server.name);
        }
        Ok(())
    }

    pub fn remove(&self, server: &McpServer) -> Result<(), CoreError> {
        let _guard = self.begin_mutation();
        let (scope, local) = server.origin.mutable_scope()?;
        {
            let _records = self.lock_records();
            let mut records = self.disabled_records()?;
            if !server.is_enabled && records.iter().any(|record| record.id == server.id) {
                records.retain(|record| record.id != server.id);
                return self.save_disabled_records(&records);
            }
        }
        self.cli.mcp_remove(&server.name, &scope, local)?;
        self.forget_project_name(&server.origin, &server.name);
        Ok(())
    }

    /// Both project-scoped origins keep a disable list Claude Code understands, but not the same
    /// one: a project's own servers are named in `disabledMcpServers`, its `.mcp.json` servers in
    /// `disabledMcpjsonServers`.
    fn set_project_disabled(
        &self,
        origin: &McpOrigin,
        name: &str,
        enabled: bool,
    ) -> Result<(), CoreError> {
        match origin {
            McpOrigin::ProjectFile { project } => {
                set_project_mcpjson_server(&self.paths.claude_json, project, name, enabled)
            }
            McpOrigin::LocalScope { project } => {
                set_project_mcp_server(&self.paths.claude_json, project, name, enabled)
            }
            _ => Ok(()),
        }
    }

    /// Best effort: a name left in the disable list its origin uses silently switches off the next
    /// server added under it, in Claude Code as well as here. Only that origin's list is touched,
    /// since a same-named server of the other origin has its own disabled state.
    fn forget_project_name(&self, origin: &McpOrigin, name: &str) {
        let _ = match origin {
            McpOrigin::ProjectFile { project } => {
                forget_project_mcpjson_server(&self.paths.claude_json, project, name)
            }
            McpOrigin::LocalScope { project } => {
                forget_project_local_server(&self.paths.claude_json, project, name)
            }
            _ => Ok(()),
        };
    }

    pub fn set_enabled(&self, server: &McpServer, enabled: bool) -> Result<(), CoreError> {
        let _guard = self.begin_mutation();
        let (scope, local) = server.origin.mutable_scope()?;
        if server.is_enabled == enabled {
            return Ok(());
        }

        // Both project-scoped origins have a disable Claude Code understands natively, so they
        // toggle a name in place instead of stashing the config and removing the server.
        if matches!(
            server.origin,
            McpOrigin::ProjectFile { .. } | McpOrigin::LocalScope { .. }
        ) {
            return self.set_project_disabled(&server.origin, &server.name, enabled);
        }

        if !enabled {
            {
                let _records = self.lock_records();
                let mut records = self.disabled_records()?;
                records.retain(|record| record.id != server.id);
                records.push(DisabledMcpServer::new(
                    &server.name,
                    server.origin.clone(),
                    server.config.clone(),
                    self.clock.now_rfc3339(),
                ));
                self.save_disabled_records(&records)?;
            }
            if let Err(error) = self.cli.mcp_remove(&server.name, &scope, local) {
                self.drop_record(&server.id);
                return Err(error);
            }
            return Ok(());
        }

        let config = {
            let _records = self.lock_records();
            self.disabled_records()?
                .into_iter()
                .find(|record| record.id == server.id)
                .map(|record| record.config)
                .unwrap_or_else(|| server.config.clone())
        };
        self.cli
            .mcp_add_json(&server.name, &encode(&config)?, &scope, local)?;
        let _records = self.lock_records();
        let mut records = self.disabled_records()?;
        records.retain(|record| record.id != server.id);
        self.save_disabled_records(&records)
    }

    /// Best effort: a record whose parking removal failed must not outlive the failure. Re-read so
    /// a record another park wrote meanwhile is carried across instead of clobbered.
    fn drop_record(&self, id: &str) {
        let _records = self.lock_records();
        if let Ok(mut records) = self.disabled_records() {
            records.retain(|record| record.id != id);
            let _ = self.save_disabled_records(&records);
        }
    }

    fn disabled_records(&self) -> Result<Vec<DisabledMcpServer>, CoreError> {
        load_json_store(&self.disabled_json_path())
    }

    fn save_disabled_records(&self, records: &[DisabledMcpServer]) -> Result<(), CoreError> {
        save_json_store(&self.disabled_json_path(), &records.to_vec())
    }

    fn disabled_json_path(&self) -> PathBuf {
        self.paths.mcp_disabled_file()
    }

    fn state(&self) -> MutexGuard<'_, StoreState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_records(&self) -> MutexGuard<'_, ()> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Held for the whole of a mutation, so a snapshot read while the CLI is still working is not
    /// trusted to prune anything.
    fn begin_mutation(&self) -> MutationGuard<'_> {
        self.state().pending_mutations += 1;
        MutationGuard { store: self }
    }
}

struct MutationGuard<'a> {
    store: &'a McpStore,
}

impl Drop for MutationGuard<'_> {
    fn drop(&mut self) {
        let now = self.store.monotonic.now();
        let mut state = self.store.state();
        state.pending_mutations -= 1;
        state.last_mutation_at = Some(now);
    }
}

fn encode(config: &McpServerConfig) -> Result<String, CoreError> {
    Ok(serde_json::to_string(config)?)
}
