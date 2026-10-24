use serde::Serialize;

use crate::cli_records::{MarketplaceRecord, McpConnector, PluginRecord};
use crate::discovery::ProjectEntry;
use crate::fettlr::UpdateFailure;
use crate::mcp::McpServer;
use crate::skills::{AccountSkillBehind, ParkedSkill, ParkedSubagent, Skill};
use crate::subagents::Subagent;
use crate::updates::UpdateReport;
use crate::usage::UsageIndex;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Snapshot {
    pub cli_available: bool,
    pub skills: Vec<Skill>,
    pub account_skills_behind: Vec<AccountSkillBehind>,
    pub parked_skills: Vec<ParkedSkill>,
    pub subagents: Vec<Subagent>,
    pub parked_subagents: Vec<ParkedSubagent>,
    pub plugins: Vec<PluginRecord>,
    pub available_plugins: Vec<PluginRecord>,
    pub marketplaces: Vec<MarketplaceRecord>,
    pub mcp_servers: Vec<McpServer>,
    pub projects: Vec<ProjectEntry>,
    /// How often Claude Code has reached for each skill and plugin, as it records it itself.
    pub usage: UsageIndex,
    pub update_report: Option<UpdateReport>,
    pub warnings: Vec<String>,
    /// What the last `update_all` could not update, cleared by the next plain refresh.
    pub update_failures: Vec<UpdateFailure>,
    /// Connectors configured on claude.ai. Fettlr has no local config for these and cannot manage
    /// them, and reading them costs a health check of every server on the machine, so they are
    /// cached from an explicit check rather than refreshed with everything else.
    pub connectors: Vec<McpConnector>,
    pub connectors_checked_at: Option<String>,
    pub last_refreshed: Option<String>,
}
