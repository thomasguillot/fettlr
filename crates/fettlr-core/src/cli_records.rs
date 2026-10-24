use std::collections::BTreeMap;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::models::{Scope, UNKNOWN_VERSION};

#[derive(Clone, Debug, Serialize)]
pub struct PluginRecord {
    pub plugin_id: String,
    pub name: String,
    pub marketplace_name: String,
    pub version: String,
    pub scope: Scope,
    pub enabled: bool,
    pub install_path: String,
    pub installed_at: Option<String>,
    pub last_updated: Option<String>,
    pub mcp_servers: Option<BTreeMap<String, McpServerConfig>>,
    pub is_available_only: bool,
    pub description: Option<String>,
}

impl PluginRecord {
    pub fn id(&self) -> String {
        format!("{}#{}", self.plugin_id, self.scope.key())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPluginRecord {
    id: Option<String>,
    plugin_id: Option<String>,
    name: Option<String>,
    description: Option<String>,
    marketplace_name: Option<String>,
    version: Option<String>,
    scope: Option<String>,
    project_path: Option<String>,
    enabled: Option<bool>,
    install_path: Option<String>,
    installed_at: Option<String>,
    last_updated: Option<String>,
    mcp_servers: Option<BTreeMap<String, McpServerConfig>>,
}

impl<'de> Deserialize<'de> for PluginRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawPluginRecord::deserialize(deserializer)?;
        let plugin_id = raw
            .id
            .or(raw.plugin_id)
            .ok_or_else(|| D::Error::missing_field("id"))?;
        if plugin_id.is_empty() {
            return Err(D::Error::custom("plugin entry is missing its identifier"));
        }

        let (derived_name, derived_marketplace) = split_plugin_id(&plugin_id);
        let install_path = raw.install_path.unwrap_or_default();
        let scope = match raw.scope.as_deref() {
            None | Some("user") => Scope::User,
            Some("project") => Scope::Project {
                path: raw.project_path.unwrap_or_default(),
            },
            Some("local") => Scope::Local {
                path: raw.project_path.unwrap_or_default(),
            },
            Some(other) => Scope::Other {
                name: other.to_string(),
            },
        };

        Ok(PluginRecord {
            name: raw.name.unwrap_or(derived_name),
            marketplace_name: raw.marketplace_name.unwrap_or(derived_marketplace),
            version: raw.version.unwrap_or_else(|| UNKNOWN_VERSION.to_string()),
            description: raw.description,
            enabled: raw.enabled.unwrap_or(false),
            is_available_only: install_path.is_empty(),
            install_path,
            scope,
            installed_at: raw.installed_at,
            last_updated: raw.last_updated,
            mcp_servers: raw.mcp_servers,
            plugin_id,
        })
    }
}

fn split_plugin_id(plugin_id: &str) -> (String, String) {
    match plugin_id.rfind('@') {
        Some(index) => (
            plugin_id[..index].to_string(),
            plugin_id[index + 1..].to_string(),
        ),
        None => (plugin_id.to_string(), String::new()),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceRecord {
    pub name: String,
    pub source: String,
    pub repo: Option<String>,
    pub url: Option<String>,
    pub install_location: String,
}

impl MarketplaceRecord {
    pub fn display_source(&self) -> &str {
        self.repo
            .as_deref()
            .or(self.url.as_deref())
            .unwrap_or(&self.source)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    /// Every key Claude Code writes that Fettlr has no model for, carried verbatim. Disabling a
    /// server stashes this copy and then has the CLI delete the original, so for a moment the copy
    /// is the only one there is: anything dropped here is dropped permanently.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PluginListPayload {
    Flat(Vec<PluginRecord>),
    Grouped {
        #[serde(default)]
        installed: Vec<PluginRecord>,
        #[serde(default)]
        available: Vec<PluginRecord>,
    },
}

pub fn parse_plugin_list(json: &str) -> Result<Vec<PluginRecord>, CoreError> {
    Ok(match plugin_payload(json)? {
        PluginListPayload::Flat(plugins) => plugins,
        PluginListPayload::Grouped {
            mut installed,
            available,
        } => {
            installed.extend(available);
            installed
        }
    })
}

/// `PluginListPayload`'s grouped variant defaults both of its arrays, so an untagged decode
/// accepts any object at all and answers "nothing installed" for it. An error object, or a shape
/// a later `claude` invents, must not read as an empty machine.
fn plugin_payload(json: &str) -> Result<PluginListPayload, CoreError> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let recognised = match &value {
        serde_json::Value::Array(_) => true,
        serde_json::Value::Object(fields) => {
            fields.contains_key("installed") || fields.contains_key("available")
        }
        _ => false,
    };
    if !recognised {
        return Err(CoreError::UnexpectedShape {
            command: "plugin list".to_string(),
            expected: "list of plugins".to_string(),
        });
    }
    Ok(serde_json::from_value(value)?)
}

pub fn parse_marketplace_list(json: &str) -> Result<Vec<MarketplaceRecord>, CoreError> {
    Ok(serde_json::from_str(json)?)
}

/// The catalogue Claude Code offers for install. It leaves out plugins already installed at user
/// scope or enabled in the current project, so it feeds Browse only and is not an update source.
/// Unlike `parse_plugin_list(..., include_available: true)` (which drops an available-only entry
/// once an installed one shares its id), this keeps every catalogue entry.
pub fn parse_plugin_catalogue(json: &str) -> Result<Vec<PluginRecord>, CoreError> {
    Ok(match plugin_payload(json)? {
        PluginListPayload::Flat(plugins) => plugins
            .into_iter()
            .filter(|plugin| plugin.is_available_only)
            .collect(),
        PluginListPayload::Grouped { available, .. } => available,
    })
}

/// One row of `claude mcp list`. That command is the only place Claude Code reports the servers it
/// holds outside the files Fettlr reads, so this is deliberately a verbatim record of what the CLI
/// printed rather than a config Fettlr could act on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpListing {
    pub name: String,
    pub target: String,
    pub status: String,
}

impl McpListing {
    /// Claude Code names a connector after the account it belongs to. Fettlr has no config for
    /// one, so the prefix is the only thing that separates them from servers it does hold.
    pub fn is_connector(&self) -> bool {
        self.name.starts_with(CONNECTOR_PREFIX)
    }

    pub fn connector_label(&self) -> &str {
        self.name
            .strip_prefix(CONNECTOR_PREFIX)
            .unwrap_or(&self.name)
    }
}

const CONNECTOR_PREFIX: &str = "claude.ai ";

/// How a connector stands. The CLI writes a sentence with a glyph in front of it; this is that
/// sentence classified far enough to badge, with the sentence itself kept alongside.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectorState {
    Connected,
    Failed,
    Pending,
    /// Claude Code has connected to this one before but does not list it now.
    Inactive,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConnector {
    pub name: String,
    pub label: String,
    /// Absent for a connector Claude Code no longer lists: there is nothing to point at.
    pub target: Option<String>,
    /// Verbatim from the CLI, so a status Fettlr cannot classify still reaches the reader.
    pub status: Option<String>,
    pub state: ConnectorState,
}

impl McpConnector {
    fn from_listing(listing: &McpListing) -> Self {
        Self {
            name: listing.name.clone(),
            label: listing.connector_label().to_string(),
            target: Some(listing.target.clone()),
            status: Some(listing.status.clone()),
            state: classify(&listing.status),
        }
    }

    fn inactive(name: &str) -> Self {
        Self {
            name: name.to_string(),
            label: name
                .strip_prefix(CONNECTOR_PREFIX)
                .unwrap_or(name)
                .to_string(),
            target: None,
            status: None,
            state: ConnectorState::Inactive,
        }
    }
}

fn classify(status: &str) -> ConnectorState {
    let lowered = status.to_lowercase();
    if lowered.contains("connected") && !lowered.contains("not connected") {
        ConnectorState::Connected
    } else if lowered.contains("fail") || lowered.contains("error") {
        ConnectorState::Failed
    } else if lowered.contains("pending") {
        ConnectorState::Pending
    } else {
        ConnectorState::Unknown
    }
}

/// Every connector Claude Code knows about, whether it is serving one right now or not. `listed`
/// is what `claude mcp list` returned; `ever_connected` is the account's own record, which holds
/// names the list has dropped. A connector that is configured but not currently running is still
/// something the reader set up and will look for.
pub fn merge_connectors(listed: &[McpListing], ever_connected: &[String]) -> Vec<McpConnector> {
    let mut connectors: Vec<McpConnector> = listed
        .iter()
        .filter(|listing| listing.is_connector())
        .map(McpConnector::from_listing)
        .collect();
    for name in ever_connected {
        if !connectors.iter().any(|connector| &connector.name == name) {
            connectors.push(McpConnector::inactive(name));
        }
    }
    connectors.sort_by(|left, right| left.label.cmp(&right.label));
    connectors
}

/// `claude mcp list` prints for people, not for programs: a preamble, then one line per server as
/// `name: target - status`. A name may contain spaces and a target may contain both, so the split
/// is on the first `": "` and the last `" - "`.
///
/// Output that yields no rows is an empty list only when the command says so in words ("No MCP
/// servers configured"). Any other row-less output is an error, because reading an unrecognised
/// shape as "you have none" is how a wrong answer gets stated confidently.
pub fn parse_mcp_list(output: &str) -> Result<Vec<McpListing>, CoreError> {
    let rows: Vec<McpListing> = output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (name, rest) = line.split_once(": ")?;
            let (target, status) = rest.rsplit_once(" - ")?;
            if name.is_empty() || target.trim().is_empty() {
                return None;
            }
            Some(McpListing {
                name: name.to_string(),
                target: target.trim().to_string(),
                status: status.trim().to_string(),
            })
        })
        .collect();

    if rows.is_empty() {
        if output
            .lines()
            .any(|line| line.trim().starts_with("No MCP servers configured"))
        {
            return Ok(rows);
        }
        return Err(CoreError::UnexpectedShape {
            command: "mcp list".to_string(),
            expected: "list of servers".to_string(),
        });
    }
    Ok(rows)
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("failed to parse CLI JSON output: {0}")]
    BadJson(#[from] serde_json::Error),
    #[error("`claude {command}` printed output that is not a {expected}")]
    UnexpectedShape { command: String, expected: String },
    #[error("failed to read a file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Fettlr cannot change a plugin installed with the {scope} scope")]
    UnsupportedScope { scope: String },
    #[error("could not find the claude command-line tool")]
    BinaryNotFound,
    #[error("the command `{command}` failed: {stderr}")]
    Cli { command: String, stderr: String },
    #[error("the command `{command}` took too long and was stopped")]
    Timeout { command: String },
    #[error("the project folder {path} no longer exists")]
    MissingProjectDirectory { path: String },
    #[error("could not find a skill folder at {path}")]
    MissingSkill { path: String },
    #[error("could not find a subagent file at {path}")]
    MissingSubagent { path: String },
    #[error("did not move it because {path} already exists")]
    OccupiedDestination { path: String },
    #[error("could not record the move; it is now at {path}: {reason}")]
    StrandedSkill { path: String, reason: String },
    #[error("copied it to {destination} but could not remove {source_path}: {reason}")]
    SourceNotRemoved {
        source_path: String,
        destination: String,
        reason: String,
    },
    #[error("{}", corrupt_store_message(path, reason, backup))]
    CorruptStore {
        path: String,
        reason: String,
        backup: Option<String>,
    },
    #[error("{path} changed while Fettlr was updating a skill; nothing was written")]
    SkillsLockChanged { path: String },
    #[error("failed to move {path} to the trash: {reason}")]
    Trash { path: String, reason: String },
    #[error("could not read {path}")]
    UnreadableRootConfig { path: String },
    #[error("could not read {path} as a Claude Code configuration file")]
    MalformedRootConfig { path: String },
    #[error("another program kept changing {path}; nothing was written")]
    ContendedRootConfig { path: String },
    #[error("this MCP server comes from the plugin {plugin_id}; change it from the plugin instead")]
    ManagedByPlugin { plugin_id: String },
    #[error("there is already an MCP server called {name} in that scope")]
    DuplicateMcpName { name: String },
    #[error("only a local-scoped MCP server can be promoted to user scope, not {origin}")]
    NotPromotable { origin: String },
    #[error("{name} is switched off; turn it on before promoting it to user scope")]
    PromoteDisabled { name: String },
    #[error(
        "removed the MCP server {name} but could not put it back; add it again with these settings: {config_json}"
    )]
    UnrestorableMcpServer { name: String, config_json: String },
    #[error("{repo} has no upstream branch configured")]
    NoUpstream { repo: String },
    #[error("{package} is not a usable registry package name")]
    InvalidPackageName { package: String },
    #[error("the registry request for {package} failed: {reason}")]
    RegistryRequestFailed { package: String, reason: String },
    #[error("the registry response for {package} could not be read")]
    RegistryMalformedResponse { package: String },
    #[error("the skill {name} was not cloned with git, so there is nothing to pull")]
    NotAGitClone { name: String },
    #[error(
        "the skill {name} belongs to the plugin {plugin_id}; change it from the plugin instead"
    )]
    SkillManagedByPlugin { name: String, plugin_id: String },
    #[error(
        "the subagent {name} belongs to the plugin {plugin_id}; change it from the plugin instead"
    )]
    SubagentManagedByPlugin { name: String, plugin_id: String },
    #[error("could not watch {path} for changes: {reason}")]
    Watch { path: String, reason: String },
}

fn corrupt_store_message(path: &str, reason: &str, backup: &Option<String>) -> String {
    match backup {
        Some(backup) => {
            format!("could not read the JSON store at {path}, kept a copy at {backup}: {reason}")
        }
        None => format!("could not read the JSON store at {path}: {reason}"),
    }
}
