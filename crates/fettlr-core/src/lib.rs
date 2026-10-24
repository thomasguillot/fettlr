mod civil_time;
mod cli;
mod cli_records;
mod discovery;
mod fettlr;
mod frontmatter;
mod json_store;
mod mcp;
mod models;
mod paths;
mod skills;
mod snapshot;
mod subagents;
pub mod testing;
mod trashing;
mod updates;
mod usage;
mod watcher;

pub use cli::{ClaudeCli, CliOutput, CliRunner, ProcessCliRunner};
pub use cli_records::{
    ConnectorState, CoreError, MarketplaceRecord, McpConnector, McpListing, McpServerConfig,
    PluginRecord, merge_connectors, parse_marketplace_list, parse_mcp_list, parse_plugin_catalogue,
    parse_plugin_list,
};
pub use discovery::{
    DiscoveredConfig, ProjectEntry, discover, ever_connected_connectors, plugin_skills,
    plugin_subagents,
};
pub use fettlr::{Fettlr, UpdateFailure, UpdateItemKind, UpdateProgress, UpdateStage};
pub use frontmatter::parse_frontmatter;
pub use json_store::{load_json_store, save_json_store};
pub use mcp::{
    McpMutating, McpOrigin, McpServer, McpStore, MonotonicClock, set_project_mcpjson_server,
};
pub use models::{Scope, UNKNOWN_VERSION, UpdateState};
pub use paths::Paths;
pub use skills::{
    AccountSkillBehind, Clock, ParkKind, ParkedSkill, ParkedSubagent, Provenance, Skill,
    SkillStore, SkillsCliOrigin, SkillsLock, account_skills_behind, read_skills_lock,
    record_skill_update, recover_interrupted_swaps, replace_skill_folder, write_skill_zip,
};
pub use snapshot::Snapshot;
pub use subagents::Subagent;
pub use trashing::{SystemTrasher, Trashing};
pub use updates::{
    CatalogueVersion, GitProbe, GitProbing, HttpRegistryClient, PackageRegistry, PackageSpec,
    PluginCatalogue, RegistryClient, StaleIds, UpdateChecker, UpdateReport, read_plugin_catalogue,
};
pub use usage::{Usage, UsageIndex};
pub use watcher::Watcher;
