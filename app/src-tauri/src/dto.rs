use std::collections::BTreeMap;

use fettlr_core::{
    ConnectorState, CoreError, MarketplaceRecord, McpConnector, McpOrigin, McpServer,
    McpServerConfig, ParkedSkill, ParkedSubagent, PluginRecord, ProjectEntry, Provenance, Scope,
    Skill, Snapshot, StaleIds, Subagent, UpdateFailure, UpdateItemKind, UpdateReport, UpdateState,
    Usage,
};
use serde::{Deserialize, Serialize};
use specta::Type;

/// What every secret value is replaced with before it leaves the shell. The real value is only ever
/// handed over one key at a time, through `reveal_secret`.
pub const MASK: &str = "•••";

/// Matches the Swift build's floor. Below it a value is far more likely to be a port or a count
/// that happens to sit in `env` than a secret worth masking.
const MIN_SECRET_LENGTH: usize = 4;
const MIN_WEAK_SECRET_LENGTH: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiError {
    pub message: String,
    pub detail: Option<String>,
}

impl UiError {
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            detail: None,
        }
    }
}

impl From<CoreError> for UiError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Cli { command, stderr } => Self {
                message: format!("The command `{command}` failed."),
                detail: Some(stderr),
            },
            // The backup path is the reader's only route back to a store Fettlr rebuilt under
            // them, so it has to survive the trip to the window.
            CoreError::CorruptStore {
                ref path,
                ref reason,
                ref backup,
            } => Self {
                message: match backup {
                    Some(backup) => format!(
                        "Could not read the store at {path}, so it was rebuilt. The old one is at {backup}."
                    ),
                    None => format!("Could not read the store at {path}."),
                },
                detail: Some(reason.clone()),
            },
            // This message is the reader's only copy of a server Fettlr removed and could not put
            // back, so the config in it has to stay retypable. Masking it per value keeps the
            // structure legible where the whole-blob needle used against CLI echoes would flatten
            // the entire settings object to one mask.
            CoreError::UnrestorableMcpServer {
                ref config_json, ..
            } => Self {
                message: error
                    .to_string()
                    .replace(config_json, &value_masked_config(config_json)),
                detail: None,
            },
            other => Self::message(other.to_string()),
        }
    }
}

/// A config re-encoded with only its env and header values, secret-bearing args and URL masked.
/// Falls back to a bare mask when it cannot be rebuilt, since losing the recovery text beats
/// printing a secret.
fn value_masked_config(config_json: &str) -> String {
    let Ok(mut config) = serde_json::from_str::<McpServerConfig>(config_json) else {
        return MASK.to_string();
    };
    config.env = config.env.as_ref().map(masked);
    config.headers = config.headers.as_ref().map(masked);
    config.args = config.args.as_deref().map(mask_args);
    config.url = config.url.as_deref().map(|url| mask_url(url).0);
    serde_json::to_string(&config).unwrap_or_else(|_| MASK.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UiScope {
    User,
    Project { path: String },
    Local { path: String },
    Other { name: String },
}

impl From<&Scope> for UiScope {
    fn from(scope: &Scope) -> Self {
        match scope {
            Scope::User => UiScope::User,
            Scope::Project { path } => UiScope::Project { path: path.clone() },
            Scope::Local { path } => UiScope::Local { path: path.clone() },
            Scope::Other { name } => UiScope::Other { name: name.clone() },
        }
    }
}

impl From<&UiScope> for Scope {
    fn from(scope: &UiScope) -> Self {
        match scope {
            UiScope::User => Scope::User,
            UiScope::Project { path } => Scope::Project { path: path.clone() },
            UiScope::Local { path } => Scope::Local { path: path.clone() },
            UiScope::Other { name } => Scope::Other { name: name.clone() },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UiProvenance {
    Local,
    GitClone,
    Plugin {
        #[serde(rename = "pluginId")]
        plugin_id: String,
    },
}

impl From<&Provenance> for UiProvenance {
    fn from(provenance: &Provenance) -> Self {
        match provenance {
            Provenance::Local => UiProvenance::Local,
            Provenance::GitClone => UiProvenance::GitClone,
            Provenance::Plugin { plugin_id } => UiProvenance::Plugin {
                plugin_id: plugin_id.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UiMcpOrigin {
    UserScope,
    LocalScope {
        project: String,
    },
    ProjectFile {
        project: String,
    },
    Plugin {
        #[serde(rename = "pluginId")]
        plugin_id: String,
    },
}

impl From<&McpOrigin> for UiMcpOrigin {
    fn from(origin: &McpOrigin) -> Self {
        match origin {
            McpOrigin::UserScope => UiMcpOrigin::UserScope,
            McpOrigin::LocalScope { project } => UiMcpOrigin::LocalScope {
                project: project.clone(),
            },
            McpOrigin::ProjectFile { project } => UiMcpOrigin::ProjectFile {
                project: project.clone(),
            },
            McpOrigin::Plugin { plugin_id } => UiMcpOrigin::Plugin {
                plugin_id: plugin_id.clone(),
            },
        }
    }
}

impl From<&UiMcpOrigin> for McpOrigin {
    fn from(origin: &UiMcpOrigin) -> Self {
        match origin {
            UiMcpOrigin::UserScope => McpOrigin::UserScope,
            UiMcpOrigin::LocalScope { project } => McpOrigin::LocalScope {
                project: project.clone(),
            },
            UiMcpOrigin::ProjectFile { project } => McpOrigin::ProjectFile {
                project: project.clone(),
            },
            UiMcpOrigin::Plugin { plugin_id } => McpOrigin::Plugin {
                plugin_id: plugin_id.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UiUpdateState {
    Unknown,
    UpToDate,
    Checking,
    UpdateAvailable { installed: String, latest: String },
    Behind { commits: u32 },
    TracksLatest,
}

impl From<&UpdateState> for UiUpdateState {
    fn from(state: &UpdateState) -> Self {
        match state {
            UpdateState::Unknown => UiUpdateState::Unknown,
            UpdateState::UpToDate => UiUpdateState::UpToDate,
            UpdateState::Checking => UiUpdateState::Checking,
            UpdateState::UpdateAvailable { installed, latest } => UiUpdateState::UpdateAvailable {
                installed: installed.clone(),
                latest: latest.clone(),
            },
            UpdateState::Behind { commits } => UiUpdateState::Behind { commits: *commits },
            UpdateState::TracksLatest => UiUpdateState::TracksLatest,
        }
    }
}

/// How often Claude Code has reached for something, as it records it in `~/.claude.json`.
/// Absent here means Claude Code holds no record at all, which is not the same as a count of zero:
/// zero is a thing installed and never used, and that is the finding worth showing.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiUsage {
    pub count: u32,
    pub last_used_at: Option<String>,
}

impl From<&Usage> for UiUsage {
    fn from(usage: &Usage) -> Self {
        Self {
            // Specta refuses u64 across the bridge, and a count that overflows u32 is not a
            // number anyone reads: saturating is honest and keeps the type precision-safe in JS.
            count: u32::try_from(usage.count).unwrap_or(u32::MAX),
            last_used_at: usage.last_used_at.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiSkill {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub scope: UiScope,
    pub provenance: UiProvenance,
    pub path: String,
    pub usage: Option<UiUsage>,
}

impl From<&Skill> for UiSkill {
    fn from(skill: &Skill) -> Self {
        Self {
            id: skill.id.clone(),
            name: skill.name.clone(),
            title: skill.title.clone(),
            summary: skill.summary.clone(),
            scope: UiScope::from(&skill.scope),
            provenance: UiProvenance::from(&skill.provenance),
            path: skill.path.display().to_string(),
            usage: None,
        }
    }
}

impl UiSkill {
    fn with_usage(mut self, usage: Option<&Usage>) -> Self {
        self.usage = usage.map(UiUsage::from);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiParkedSkill {
    pub id: String,
    pub name: String,
    pub scope_key: String,
    pub original_path: String,
    pub parked_path: String,
    pub parked_at: String,
}

impl From<&ParkedSkill> for UiParkedSkill {
    fn from(parked: &ParkedSkill) -> Self {
        Self {
            id: parked.parked_path.clone(),
            name: parked.name.clone(),
            scope_key: parked.scope_key.clone(),
            original_path: parked.original_path.clone(),
            parked_path: parked.parked_path.clone(),
            parked_at: parked.parked_at.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiSubagent {
    pub id: String,
    pub name: String,
    pub summary: Option<String>,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub effort: Option<String>,
    pub permission_mode: Option<String>,
    pub max_turns: Option<u32>,
    pub memory: Option<String>,
    pub scope: UiScope,
    pub provenance: UiProvenance,
    pub path: String,
}

impl From<&Subagent> for UiSubagent {
    fn from(subagent: &Subagent) -> Self {
        Self {
            id: subagent.id.clone(),
            name: subagent.name.clone(),
            summary: subagent.summary.clone(),
            model: subagent.model.clone(),
            tools: subagent.tools.clone(),
            effort: subagent.effort.clone(),
            permission_mode: subagent.permission_mode.clone(),
            max_turns: subagent.max_turns,
            memory: subagent.memory.clone(),
            scope: UiScope::from(&subagent.scope),
            provenance: UiProvenance::from(&subagent.provenance),
            path: subagent.path.display().to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiParkedSubagent {
    pub id: String,
    pub name: String,
    pub scope_key: String,
    pub original_path: String,
    pub parked_path: String,
    pub parked_at: String,
}

impl From<&ParkedSubagent> for UiParkedSubagent {
    fn from(parked: &ParkedSubagent) -> Self {
        Self {
            id: parked.parked_path.clone(),
            name: parked.name.clone(),
            scope_key: parked.scope_key.clone(),
            original_path: parked.original_path.clone(),
            parked_path: parked.parked_path.clone(),
            parked_at: parked.parked_at.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiPlugin {
    pub id: String,
    pub plugin_id: String,
    pub name: String,
    pub description: Option<String>,
    pub marketplace_name: String,
    pub version: String,
    pub scope: UiScope,
    pub enabled: bool,
    pub install_path: String,
    pub installed_at: Option<String>,
    pub last_updated: Option<String>,
    pub is_available_only: bool,
    /// Names only: a plugin's bundled MCP configs carry the same env and header values the shell
    /// masks everywhere else.
    pub mcp_server_names: Vec<String>,
    pub usage: Option<UiUsage>,
}

impl From<&PluginRecord> for UiPlugin {
    fn from(plugin: &PluginRecord) -> Self {
        Self {
            id: plugin.id(),
            plugin_id: plugin.plugin_id.clone(),
            name: plugin.name.clone(),
            description: plugin.description.clone(),
            marketplace_name: plugin.marketplace_name.clone(),
            version: plugin.version.clone(),
            scope: UiScope::from(&plugin.scope),
            enabled: plugin.enabled,
            install_path: plugin.install_path.clone(),
            installed_at: plugin.installed_at.clone(),
            last_updated: plugin.last_updated.clone(),
            is_available_only: plugin.is_available_only,
            mcp_server_names: plugin
                .mcp_servers
                .as_ref()
                .map(|servers| servers.keys().cloned().collect())
                .unwrap_or_default(),
            usage: None,
        }
    }
}

impl UiPlugin {
    fn with_usage(mut self, usage: Option<&Usage>) -> Self {
        self.usage = usage.map(UiUsage::from);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiMarketplace {
    pub name: String,
    pub source: String,
    pub repo: Option<String>,
    pub url: Option<String>,
    pub install_location: String,
    pub display_source: String,
}

impl From<&MarketplaceRecord> for UiMarketplace {
    fn from(marketplace: &MarketplaceRecord) -> Self {
        Self {
            name: marketplace.name.clone(),
            source: marketplace.source.clone(),
            repo: marketplace.repo.clone(),
            url: marketplace.url.clone(),
            install_location: marketplace.install_location.clone(),
            display_source: marketplace.display_source().to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiMcpConfig {
    pub kind: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub masked_args: Vec<u32>,
    pub url: Option<String>,
    pub url_masked: bool,
    pub env: BTreeMap<String, String>,
    pub env_keys: Vec<String>,
    pub headers: BTreeMap<String, String>,
    pub header_keys: Vec<String>,
}

impl From<&McpServerConfig> for UiMcpConfig {
    fn from(config: &McpServerConfig) -> Self {
        let env = config.env.as_ref().map(masked).unwrap_or_default();
        let headers = config.headers.as_ref().map(masked).unwrap_or_default();
        let original_args = config.args.clone().unwrap_or_default();
        let args = mask_args(&original_args);
        let masked_args = args
            .iter()
            .zip(&original_args)
            .enumerate()
            .filter(|(_, (masked, original))| masked != original)
            .map(|(index, _)| index as u32)
            .collect();
        let url = config.url.as_deref().map(|url| mask_url(url).0);
        Self {
            kind: config.kind.clone(),
            command: config.command.clone(),
            args,
            masked_args,
            url_masked: url != config.url,
            url,
            env_keys: env.keys().cloned().collect(),
            header_keys: headers.keys().cloned().collect(),
            env,
            headers,
        }
    }
}

const SENSITIVE_NAMES: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "pwd",
    "pass",
    "key",
    "apikey",
    "auth",
    "authorization",
    "credential",
    "credentials",
    "bearer",
    "cookie",
    "pat",
    "accesstoken",
];

fn name_segments(name: &str) -> Vec<String> {
    let mut segments = Vec::new();
    for part in name.trim_start_matches('-').split(['-', '_', '.']) {
        let chars: Vec<char> = part.chars().collect();
        let mut current = String::new();
        for (index, &c) in chars.iter().enumerate() {
            let starts_word = c.is_uppercase()
                && index > 0
                && (chars[index - 1].is_lowercase()
                    || (chars[index - 1].is_uppercase()
                        && chars.get(index + 1).is_some_and(|next| next.is_lowercase())));
            if starts_word && !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
            current.extend(c.to_lowercase());
        }
        if !current.is_empty() {
            segments.push(current);
        }
    }
    segments
}

fn is_sensitive_name(name: &str) -> bool {
    let segments = name_segments(name);
    match (segments.first(), segments.last()) {
        (Some(first), Some(last)) => first != "no" && SENSITIVE_NAMES.contains(&last.as_str()),
        _ => false,
    }
}

fn is_flag(arg: &str) -> bool {
    arg.starts_with('-') && arg.len() > 1
}

type Found = (String, Vec<Secret>);

/// A substring taken out of an argument or URL. `weak` marks a query value under a key that does
/// not look sensitive, which is as likely to be a format name as a secret.
struct Secret {
    value: String,
    weak: bool,
}

impl Secret {
    fn strong(value: &str) -> Self {
        Self {
            value: value.to_string(),
            weak: false,
        }
    }
}

fn is_url(text: &str) -> bool {
    text.split_once("://").is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

/// `Name:value` or `Name: value` with a sensitive `Name`, the shape of an HTTP header passed as an
/// argument. The separator keeps its original spacing in the masked form.
fn masked_header(text: &str) -> Option<Found> {
    let (name, rest) = text.split_once(':')?;
    if name.is_empty() || name.contains(char::is_whitespace) || !is_sensitive_name(name) {
        return None;
    }
    let value = rest.trim_start();
    if value.is_empty() {
        return None;
    }
    let separator = &rest[..rest.len() - value.len()];
    Some((
        format!("{name}:{separator}{MASK}"),
        vec![Secret::strong(value)],
    ))
}

fn masked_shape(text: &str) -> Option<Found> {
    if is_url(text) {
        let (masked, secrets) = mask_url_tagged(text);
        return (masked != text).then_some((masked, secrets));
    }
    masked_header(text)
}

/// The masked form of `args[index]` and the secret substrings taken out of it, or `None` when
/// nothing in it needs hiding.
fn mask_arg(args: &[String], index: usize) -> Option<Found> {
    let arg = args[index].as_str();
    if arg.is_empty() {
        return None;
    }
    if let Some(previous) = index.checked_sub(1).map(|i| args[i].as_str())
        && is_flag(previous)
        && !previous.contains('=')
        && is_sensitive_name(previous)
        && !arg.starts_with('-')
    {
        return Some((MASK.to_string(), vec![Secret::strong(arg)]));
    }
    if is_flag(arg) {
        let (name, value) = arg.split_once('=')?;
        return mask_named_value(name, value);
    }
    if let Some(found) = masked_shape(arg) {
        return Some(found);
    }
    let (name, value) = arg.split_once('=')?;
    if name.contains(char::is_whitespace) {
        return None;
    }
    mask_named_value(name, value)
}

fn mask_named_value(name: &str, value: &str) -> Option<Found> {
    if name.is_empty() || value.is_empty() {
        return None;
    }
    if is_sensitive_name(name) {
        return Some((format!("{name}={MASK}"), vec![Secret::strong(value)]));
    }
    masked_shape(value).map(|(masked, secrets)| (format!("{name}={masked}"), secrets))
}

pub fn mask_args(args: &[String]) -> Vec<String> {
    (0..args.len())
        .map(|index| match mask_arg(args, index) {
            Some((masked, _)) => masked,
            None => args[index].clone(),
        })
        .collect()
}

fn arg_secrets(args: &[String]) -> Vec<Secret> {
    (0..args.len())
        .filter_map(|index| mask_arg(args, index))
        .flat_map(|(_, secrets)| secrets)
        .collect()
}

/// The URL with its userinfo password and every query value masked, plus the substrings removed.
/// Scheme, host, path and fragment are left as they were.
pub fn mask_url(url: &str) -> (String, Vec<String>) {
    let (masked, secrets) = mask_url_tagged(url);
    (
        masked,
        secrets.into_iter().map(|secret| secret.value).collect(),
    )
}

fn mask_url_tagged(url: &str) -> (String, Vec<Secret>) {
    let mut secrets = Vec::new();
    let (head, fragment) = match url.find('#') {
        Some(at) => url.split_at(at),
        None => (url, ""),
    };
    let (base, query) = match head.find('?') {
        Some(at) => (&head[..at], Some(&head[at + 1..])),
        None => (head, None),
    };
    let mut out = String::with_capacity(url.len());
    match base.split_once("://") {
        Some((scheme, rest)) => {
            let authority_end = rest.find('/').unwrap_or(rest.len());
            let (authority, path) = rest.split_at(authority_end);
            out.push_str(scheme);
            out.push_str("://");
            match authority.rfind('@') {
                Some(at) => {
                    let (userinfo, host) = authority.split_at(at);
                    match userinfo.split_once(':') {
                        Some((user, password)) if !password.is_empty() => {
                            secrets.push(Secret::strong(password));
                            out.push_str(&format!("{user}:{MASK}"));
                        }
                        _ => out.push_str(userinfo),
                    }
                    out.push_str(host);
                }
                None => out.push_str(authority),
            }
            out.push_str(path);
        }
        None => out.push_str(base),
    }
    if let Some(query) = query {
        out.push('?');
        let pairs: Vec<String> = query
            .split('&')
            .map(|pair| match pair.split_once('=') {
                Some((key, value)) if !value.is_empty() => {
                    secrets.push(Secret {
                        value: value.to_string(),
                        weak: !is_sensitive_name(key),
                    });
                    format!("{key}={MASK}")
                }
                _ => pair.to_string(),
            })
            .collect();
        out.push_str(&pairs.join("&"));
    }
    out.push_str(fragment);
    (out, secrets)
}

fn masked(values: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    values
        .keys()
        .map(|key| (key.clone(), MASK.to_string()))
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiMcpServer {
    pub id: String,
    pub name: String,
    pub origin: UiMcpOrigin,
    pub transport: String,
    pub is_enabled: bool,
    pub config: UiMcpConfig,
}

impl From<&McpServer> for UiMcpServer {
    fn from(server: &McpServer) -> Self {
        Self {
            id: server.id.clone(),
            name: server.name.clone(),
            origin: UiMcpOrigin::from(&server.origin),
            transport: server.transport.clone(),
            is_enabled: server.is_enabled,
            config: UiMcpConfig::from(&server.config),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiProject {
    pub path: String,
    pub exists: bool,
    pub local_mcp_server_names: Vec<String>,
    pub enabled_mcpjson_servers: Vec<String>,
    pub disabled_mcpjson_servers: Vec<String>,
}

impl From<&ProjectEntry> for UiProject {
    fn from(project: &ProjectEntry) -> Self {
        Self {
            path: project.path.clone(),
            exists: project.exists,
            local_mcp_server_names: project.local_mcp_servers.keys().cloned().collect(),
            enabled_mcpjson_servers: project.enabled_mcpjson_servers.clone(),
            disabled_mcpjson_servers: project.disabled_mcpjson_servers.clone(),
        }
    }
}

/// The ids the core calls stale, carried alongside the states so the window lists and counts
/// exactly what an update run would work through instead of reading the states itself.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiStaleIds {
    pub marketplaces: Vec<String>,
    pub plugins: Vec<String>,
    pub skills: Vec<String>,
    pub mcp_servers: Vec<String>,
}

impl From<StaleIds> for UiStaleIds {
    fn from(stale: StaleIds) -> Self {
        Self {
            marketplaces: stale.marketplaces,
            plugins: stale.plugins,
            skills: stale.skills,
            mcp_servers: stale.mcp_servers,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiUpdateReport {
    pub marketplaces: BTreeMap<String, UiUpdateState>,
    pub plugins: BTreeMap<String, UiUpdateState>,
    pub skills: BTreeMap<String, UiUpdateState>,
    pub mcp_servers: BTreeMap<String, UiUpdateState>,
    pub stale: UiStaleIds,
    pub checked_at: String,
}

impl From<&UpdateReport> for UiUpdateReport {
    fn from(report: &UpdateReport) -> Self {
        Self {
            marketplaces: convert_states(&report.marketplaces),
            plugins: convert_states(&report.plugins),
            skills: convert_states(&report.skills),
            mcp_servers: convert_states(&report.mcp_servers),
            stale: UiStaleIds::from(report.stale_ids()),
            checked_at: report.checked_at.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UiUpdateItemKind {
    Marketplace,
    Plugin,
    Skill,
}

impl From<UpdateItemKind> for UiUpdateItemKind {
    fn from(kind: UpdateItemKind) -> Self {
        match kind {
            UpdateItemKind::Marketplace => UiUpdateItemKind::Marketplace,
            UpdateItemKind::Plugin => UiUpdateItemKind::Plugin,
            UpdateItemKind::Skill => UiUpdateItemKind::Skill,
        }
    }
}

/// One item an update run could not update. The window words the sentence itself, so a reworded
/// core message can never be mistaken for a success.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiUpdateFailure {
    pub kind: UiUpdateItemKind,
    pub name: String,
    pub error: String,
}

impl From<&UpdateFailure> for UiUpdateFailure {
    fn from(failure: &UpdateFailure) -> Self {
        Self {
            kind: UiUpdateItemKind::from(failure.kind),
            name: failure.name.clone(),
            error: failure.error.clone(),
        }
    }
}

fn convert_states(states: &BTreeMap<String, UpdateState>) -> BTreeMap<String, UiUpdateState> {
    states
        .iter()
        .map(|(key, state)| (key.clone(), UiUpdateState::from(state)))
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiSnapshot {
    pub cli_available: bool,
    pub skills: Vec<UiSkill>,
    pub parked_skills: Vec<UiParkedSkill>,
    pub subagents: Vec<UiSubagent>,
    pub parked_subagents: Vec<UiParkedSubagent>,
    pub plugins: Vec<UiPlugin>,
    pub available_plugins: Vec<UiPlugin>,
    pub marketplaces: Vec<UiMarketplace>,
    pub mcp_servers: Vec<UiMcpServer>,
    pub projects: Vec<UiProject>,
    pub update_report: Option<UiUpdateReport>,
    pub warnings: Vec<String>,
    pub update_failures: Vec<UiUpdateFailure>,
    pub last_refreshed: Option<String>,
    pub connectors: Vec<UiConnector>,
    pub connectors_checked_at: Option<String>,
}

/// A connector configured on the reader's claude.ai account. Fettlr has no local config for one
/// and cannot change it, so this carries only what `claude mcp list` printed.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiConnector {
    pub name: String,
    pub label: String,
    pub target: Option<String>,
    /// Verbatim from the CLI, so a status Fettlr could not classify still reaches the reader.
    pub status: Option<String>,
    pub state: UiConnectorState,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UiConnectorState {
    Connected,
    Failed,
    Pending,
    Inactive,
    Unknown,
}

impl From<&ConnectorState> for UiConnectorState {
    fn from(state: &ConnectorState) -> Self {
        match state {
            ConnectorState::Connected => UiConnectorState::Connected,
            ConnectorState::Failed => UiConnectorState::Failed,
            ConnectorState::Pending => UiConnectorState::Pending,
            ConnectorState::Inactive => UiConnectorState::Inactive,
            ConnectorState::Unknown => UiConnectorState::Unknown,
        }
    }
}

impl From<&McpConnector> for UiConnector {
    fn from(connector: &McpConnector) -> Self {
        Self {
            name: connector.name.clone(),
            label: connector.label.clone(),
            target: connector.target.clone(),
            status: connector.status.clone(),
            state: UiConnectorState::from(&connector.state),
        }
    }
}

impl From<Snapshot> for UiSnapshot {
    fn from(snapshot: Snapshot) -> Self {
        let secrets = secret_values(
            snapshot
                .mcp_servers
                .iter()
                .map(|server| &server.config)
                .chain(
                    snapshot
                        .plugins
                        .iter()
                        .chain(snapshot.available_plugins.iter())
                        .filter_map(|plugin| plugin.mcp_servers.as_ref())
                        .flat_map(|servers| servers.values()),
                ),
        );
        Self {
            cli_available: snapshot.cli_available,
            skills: snapshot
                .skills
                .iter()
                .map(|skill| UiSkill::from(skill).with_usage(snapshot.usage.for_skill(skill)))
                .collect(),
            parked_skills: snapshot
                .parked_skills
                .iter()
                .map(UiParkedSkill::from)
                .collect(),
            subagents: snapshot.subagents.iter().map(UiSubagent::from).collect(),
            parked_subagents: snapshot
                .parked_subagents
                .iter()
                .map(UiParkedSubagent::from)
                .collect(),
            plugins: snapshot
                .plugins
                .iter()
                .map(|plugin| {
                    UiPlugin::from(plugin).with_usage(snapshot.usage.for_plugin(&plugin.plugin_id))
                })
                .collect(),
            available_plugins: snapshot
                .available_plugins
                .iter()
                .map(UiPlugin::from)
                .collect(),
            marketplaces: snapshot
                .marketplaces
                .iter()
                .map(UiMarketplace::from)
                .collect(),
            mcp_servers: snapshot.mcp_servers.iter().map(UiMcpServer::from).collect(),
            projects: snapshot.projects.iter().map(UiProject::from).collect(),
            update_report: snapshot.update_report.as_ref().map(UiUpdateReport::from),
            warnings: snapshot
                .warnings
                .into_iter()
                .map(|warning| redact(warning, &secrets))
                .collect(),
            update_failures: snapshot
                .update_failures
                .iter()
                .map(|failure| UiUpdateFailure {
                    error: redact(failure.error.clone(), &secrets),
                    ..UiUpdateFailure::from(failure)
                })
                .collect(),
            last_refreshed: snapshot.last_refreshed,
            connectors: snapshot.connectors.iter().map(UiConnector::from).collect(),
            connectors_checked_at: snapshot.connectors_checked_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiMcpConfigInput {
    pub kind: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub env: Option<BTreeMap<String, String>>,
    pub headers: Option<BTreeMap<String, String>>,
}

impl UiMcpConfigInput {
    /// A form prefilled from `UiMcpConfig` carries the mask, not the value, so writing the input
    /// back unchanged would replace a real secret with `•••`. The shell cannot tell that apart from
    /// someone deliberately typing the mask, so it refuses either and says how to get the value.
    pub fn to_config(&self) -> Result<McpServerConfig, UiError> {
        reject_masked_values(self.env.as_ref(), "environment variable")?;
        reject_masked_values(self.headers.as_ref(), "header")?;
        if let Some(index) = self
            .args
            .iter()
            .flatten()
            .position(|arg| arg.contains(MASK))
        {
            return Err(still_hidden(&format!("Argument {}", index + 1)));
        }
        if self.url.as_deref().is_some_and(|url| url.contains(MASK)) {
            return Err(still_hidden("The URL"));
        }
        Ok(McpServerConfig {
            kind: self.kind.clone().filter(|kind| !kind.is_empty()),
            command: self.command.clone().filter(|value| !value.is_empty()),
            args: self.args.clone().filter(|args| !args.is_empty()),
            env: self.env.clone().filter(|env| !env.is_empty()),
            url: self.url.clone().filter(|value| !value.is_empty()),
            headers: self.headers.clone().filter(|headers| !headers.is_empty()),
            // The window is never shown the keys Fettlr has no model for, so it cannot send them
            // back. An edit carries them across from the server it is editing.
            extra: Default::default(),
        })
    }
}

fn still_hidden(subject: &str) -> UiError {
    UiError {
        message: format!("{subject} is still hidden, so Fettlr did not save it."),
        detail: Some(
            "Reveal the value first, then save, or the real one would be overwritten.".to_string(),
        ),
    }
}

fn reject_masked_values(
    values: Option<&BTreeMap<String, String>>,
    label: &str,
) -> Result<(), UiError> {
    let Some(values) = values else {
        return Ok(());
    };
    match values.iter().find(|(_, value)| *value == MASK) {
        Some((key, _)) => Err(UiError {
            message: format!("The {label} {key} is still hidden, so Fettlr did not save it."),
            detail: Some(
                "Reveal the value first, then save, or the real one would be overwritten."
                    .to_string(),
            ),
        }),
        None => Ok(()),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiMcpInput {
    pub name: String,
    pub origin: UiMcpOrigin,
    pub config: UiMcpConfigInput,
}

/// Every env and header value, and every secret taken out of args and URLs, in a set of configs,
/// longest first so that a secret containing another one is masked whole rather than leaving its
/// tail behind.
///
/// Each value is listed twice, raw and JSON-escaped, and the whole encoded config leads the list:
/// the CLI echoes back the JSON it was handed, so a value carrying a quote, a backslash or a
/// newline never appears in its raw form there and a literal match alone would miss it entirely.
/// Values shorter than `MIN_SECRET_LENGTH` are dropped, since masking every "1" and "30" in a
/// config turns unrelated error text into noise. A query value under a key that does not look
/// sensitive must reach `MIN_WEAK_SECRET_LENGTH`, because a value like `json` would otherwise mask
/// `.mcp.json` in an unrelated warning.
pub fn secret_values<'a>(configs: impl Iterator<Item = &'a McpServerConfig>) -> Vec<String> {
    let mut values: Vec<String> = Vec::new();
    for config in configs {
        if let Ok(encoded) = serde_json::to_string(config) {
            values.push(encoded);
        }
        for value in config
            .env
            .iter()
            .chain(config.headers.iter())
            .flat_map(|values| values.values())
        {
            if let Some(escaped) = json_escaped(value) {
                values.push(escaped);
            }
            values.push(value.clone());
        }
        let from_args = config.args.iter().flat_map(|args| arg_secrets(args));
        let from_url = config.url.iter().flat_map(|url| mask_url_tagged(url).1);
        for value in from_args
            .chain(from_url)
            .filter(|secret| !secret.weak || secret.value.chars().count() >= MIN_WEAK_SECRET_LENGTH)
            .map(|secret| secret.value)
        {
            if let Some(escaped) = json_escaped(&value) {
                values.push(escaped);
            }
            values.push(value);
        }
    }
    values.retain(|value| value.chars().count() >= MIN_SECRET_LENGTH);
    values.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    values.dedup();
    values
}

/// The value as it appears inside a JSON string, or `None` when JSON encoding leaves it unchanged
/// and the raw form already covers it.
fn json_escaped(value: &str) -> Option<String> {
    let encoded = serde_json::to_string(value).ok()?;
    let inner = encoded.strip_prefix('"')?.strip_suffix('"')?;
    (inner != value).then(|| inner.to_string())
}

/// `claude mcp get` and `claude plugin details` print a human-readable block Fettlr shows as-is,
/// and a config's env and header values are in it. Substituting the values Fettlr already holds
/// keeps the masking correct however the CLI decides to lay that block out.
///
/// The needles are re-sorted longest first here rather than trusted to arrive that way: masking a
/// short value first would leave the tail of any longer one that contains it on screen.
pub fn redact(output: String, secrets: &[String]) -> String {
    let mut needles: Vec<&str> = secrets.iter().map(String::as_str).collect();
    needles.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    needles
        .into_iter()
        .fold(output, |redacted, secret| redacted.replace(secret, MASK))
}

/// The real value behind one masked field. `key` names the env or header entry, or the argument
/// index for `Args`; `Url` ignores it. For `Args` and `Url`, `expected` must equal the masked text
/// the config shows now, so a reveal never hands back a different argument than the one the window
/// displayed.
pub fn reveal_field(
    name: &str,
    config: &McpServerConfig,
    field: UiSecretField,
    key: &str,
    expected: Option<&str>,
) -> Result<String, UiError> {
    let gone = || {
        UiError::message(format!(
            "The MCP server {name} no longer has a value for {key}."
        ))
    };
    let moved = || {
        UiError::message(format!(
            "The MCP server {name} changed since it was shown, so Fettlr did not reveal that value."
        ))
    };
    match field {
        UiSecretField::Env => config.env.as_ref(),
        UiSecretField::Headers => config.headers.as_ref(),
        UiSecretField::Args => {
            let index: usize = key
                .parse()
                .map_err(|_| UiError::message(format!("{key} is not an argument number.")))?;
            let args = config.args.as_deref().unwrap_or_default();
            let value = args.get(index).ok_or_else(|| {
                UiError::message(format!(
                    "The MCP server {name} has no argument number {key}."
                ))
            })?;
            let current = mask_arg(args, index).map_or_else(|| value.clone(), |(masked, _)| masked);
            return if expected == Some(current.as_str()) {
                Ok(value.clone())
            } else {
                Err(moved())
            };
        }
        UiSecretField::Url => {
            let url = config.url.as_ref().ok_or_else(|| {
                UiError::message(format!("The MCP server {name} no longer has a URL."))
            })?;
            return if expected == Some(mask_url(url).0.as_str()) {
                Ok(url.clone())
            } else {
                Err(moved())
            };
        }
    }
    .and_then(|values| values.get(key))
    .cloned()
    .ok_or_else(gone)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UiSecretField {
    Env,
    Headers,
    Args,
    Url,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UiColorMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UiPalette {
    #[default]
    Default,
    HighContrast,
    Dracula,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UiPrefs {
    pub binary_override: Option<String>,
    /// `None` means manual checks only.
    pub check_interval_hours: Option<u32>,
    pub auto_check_for_updates: bool,
    pub color_mode: UiColorMode,
    pub palette: UiPalette,
    pub launch_at_login: bool,
    pub confirm_mcp_promotion: bool,
    pub sidebar_collapsed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(json: serde_json::Value) -> McpServerConfig {
        serde_json::from_value(json).expect("decode config")
    }

    #[test]
    fn env_and_header_values_are_masked_and_their_keys_listed() {
        let ui = UiMcpConfig::from(&config(serde_json::json!({
            "url": "https://example.com",
            "env": {"TOKEN": "sk-secret", "REGION": "eu"},
            "headers": {"Authorization": "Bearer sk-secret"}
        })));

        assert_eq!(ui.env.get("TOKEN"), Some(&MASK.to_string()));
        assert_eq!(ui.env.get("REGION"), Some(&MASK.to_string()));
        assert_eq!(ui.headers.get("Authorization"), Some(&MASK.to_string()));
        assert_eq!(ui.env_keys, vec!["REGION".to_string(), "TOKEN".to_string()]);
        assert_eq!(ui.header_keys, vec!["Authorization".to_string()]);
    }

    #[test]
    fn a_serialised_mcp_server_never_carries_a_real_secret() {
        let server = McpServer::new(
            "context7",
            McpOrigin::UserScope,
            config(serde_json::json!({"command": "npx", "env": {"TOKEN": "sk-secret"}})),
            true,
        );
        let json = serde_json::to_string(&UiMcpServer::from(&server)).expect("serialises");
        assert!(!json.contains("sk-secret"));
        assert!(json.contains(MASK));
    }

    #[test]
    fn a_plugin_reports_its_mcp_server_names_without_their_configs() {
        let plugin: PluginRecord = serde_json::from_value(serde_json::json!({
            "id": "helper@market",
            "installPath": "/tmp/helper",
            "mcpServers": {"inner": {"command": "npx", "env": {"TOKEN": "sk-secret"}}}
        }))
        .expect("decode plugin");

        let ui = UiPlugin::from(&plugin);
        assert_eq!(ui.mcp_server_names, vec!["inner".to_string()]);
        let json = serde_json::to_string(&ui).expect("serialises");
        assert!(!json.contains("sk-secret"));
    }

    #[test]
    fn a_project_reports_its_local_server_names_without_their_configs() {
        let entry = ProjectEntry {
            path: "/tmp/project".to_string(),
            exists: true,
            has_trust_dialog_accepted: true,
            local_mcp_servers: BTreeMap::from([(
                "local".to_string(),
                config(serde_json::json!({"command": "npx", "env": {"TOKEN": "sk-secret"}})),
            )]),
            enabled_mcpjson_servers: Vec::new(),
            disabled_mcpjson_servers: Vec::new(),
            disabled_mcp_servers: Vec::new(),
        };

        let json = serde_json::to_string(&UiProject::from(&entry)).expect("serialises");
        assert!(!json.contains("sk-secret"));
        assert!(json.contains("local"));
    }

    #[test]
    fn every_dto_key_reaches_typescript_in_camel_case() {
        let ui = UiSnapshot::from(Snapshot::default());
        let json = serde_json::to_value(&ui).expect("serialises");
        let object = json.as_object().expect("an object");
        assert!(object.contains_key("cliAvailable"));
        assert!(object.contains_key("parkedSkills"));
        assert!(object.contains_key("availablePlugins"));
        assert!(object.contains_key("mcpServers"));
        assert!(object.contains_key("updateReport"));
        assert!(object.contains_key("lastRefreshed"));
        assert!(object.keys().all(|key| !key.contains('_')));
    }

    #[test]
    fn a_plugin_record_is_re_keyed_from_snake_case() {
        let plugin: PluginRecord = serde_json::from_value(serde_json::json!({
            "id": "helper@market",
            "installPath": "/tmp/helper",
            "installedAt": "2026-08-23T10:00:00Z"
        }))
        .expect("decode plugin");

        let json = serde_json::to_value(UiPlugin::from(&plugin)).expect("serialises");
        let object = json.as_object().expect("an object");
        assert!(object.contains_key("pluginId"));
        assert!(object.contains_key("marketplaceName"));
        assert!(object.contains_key("installPath"));
        assert!(object.contains_key("isAvailableOnly"));
        assert!(object.keys().all(|key| !key.contains('_')));
    }

    #[test]
    fn a_scope_round_trips_through_its_dto() {
        for scope in [
            Scope::User,
            Scope::Project {
                path: "/tmp/project".to_string(),
            },
            Scope::Local {
                path: "/tmp/project".to_string(),
            },
            Scope::Other {
                name: "synced".to_string(),
            },
        ] {
            let ui = UiScope::from(&scope);
            assert_eq!(Scope::from(&ui).key(), scope.key());
        }
    }

    #[test]
    fn an_mcp_origin_round_trips_through_its_dto() {
        for origin in [
            McpOrigin::UserScope,
            McpOrigin::LocalScope {
                project: "/tmp/project".to_string(),
            },
            McpOrigin::ProjectFile {
                project: "/tmp/project".to_string(),
            },
            McpOrigin::Plugin {
                plugin_id: "helper@market".to_string(),
            },
        ] {
            let ui = UiMcpOrigin::from(&origin);
            assert_eq!(McpOrigin::from(&ui), origin);
        }
    }

    #[test]
    fn a_config_input_drops_empty_collections_rather_than_writing_them() {
        let input = UiMcpConfigInput {
            kind: Some(String::new()),
            command: Some("npx".to_string()),
            args: Some(Vec::new()),
            url: None,
            env: Some(BTreeMap::new()),
            headers: None,
        };
        let config = input.to_config().expect("no masked values");
        assert_eq!(config.command.as_deref(), Some("npx"));
        assert_eq!(config.kind, None);
        assert_eq!(config.args, None);
        assert_eq!(config.env, None);
    }

    fn input_with(
        env: Option<BTreeMap<String, String>>,
        headers: Option<BTreeMap<String, String>>,
    ) -> UiMcpConfigInput {
        UiMcpConfigInput {
            kind: None,
            command: Some("npx".to_string()),
            args: None,
            url: None,
            env,
            headers,
        }
    }

    #[test]
    fn a_still_masked_env_value_is_refused_rather_than_written() {
        let input = input_with(
            Some(BTreeMap::from([("TOKEN".to_string(), MASK.to_string())])),
            None,
        );
        let error = input.to_config().expect_err("refuses the mask");
        assert!(error.message.contains("TOKEN"));
        assert!(error.detail.is_some());
    }

    #[test]
    fn a_still_masked_header_value_is_refused_rather_than_written() {
        let input = input_with(
            None,
            Some(BTreeMap::from([(
                "Authorization".to_string(),
                MASK.to_string(),
            )])),
        );
        let error = input.to_config().expect_err("refuses the mask");
        assert!(error.message.contains("Authorization"));
    }

    #[test]
    fn a_masked_value_beside_real_ones_still_refuses_the_whole_write() {
        let input = input_with(
            Some(BTreeMap::from([
                ("REGION".to_string(), "eu".to_string()),
                ("TOKEN".to_string(), MASK.to_string()),
            ])),
            None,
        );
        assert!(input.to_config().is_err());
    }

    #[test]
    fn real_values_pass_through_untouched() {
        let input = input_with(
            Some(BTreeMap::from([(
                "TOKEN".to_string(),
                "sk-real".to_string(),
            )])),
            Some(BTreeMap::from([(
                "Authorization".to_string(),
                "Bearer sk-real".to_string(),
            )])),
        );
        let config = input.to_config().expect("real values are accepted");
        assert_eq!(
            config.env.expect("env").get("TOKEN"),
            Some(&"sk-real".to_string())
        );
        assert_eq!(
            config.headers.expect("headers").get("Authorization"),
            Some(&"Bearer sk-real".to_string())
        );
    }

    #[test]
    fn a_corrupt_store_error_keeps_the_backup_path_the_reader_needs() {
        let error = UiError::from(CoreError::CorruptStore {
            path: "/Support/parked.json".to_string(),
            reason: "expected value at line 1".to_string(),
            backup: Some("/Support/parked.json.corrupt-20260823".to_string()),
        });
        assert!(error.message.contains("/Support/parked.json"));
        assert!(
            error
                .message
                .contains("/Support/parked.json.corrupt-20260823")
        );
        assert_eq!(error.detail.as_deref(), Some("expected value at line 1"));
    }

    #[test]
    fn a_corrupt_store_error_without_a_backup_says_only_what_is_true() {
        let error = UiError::from(CoreError::CorruptStore {
            path: "/Support/parked.json".to_string(),
            reason: "expected value at line 1".to_string(),
            backup: None,
        });
        assert!(error.message.contains("/Support/parked.json"));
        assert!(!error.message.contains("corrupt-"));
        assert_eq!(error.detail.as_deref(), Some("expected value at line 1"));
    }

    #[test]
    fn secret_values_are_gathered_longest_first_and_never_empty() {
        let config = config(serde_json::json!({
            "env": {"SHORT": "abc", "LONG": "abc-def", "BLANK": ""},
            "headers": {"Authorization": "Bearer abc-def-ghi"}
        }));
        let values = secret_values(std::iter::once(&config));
        assert_eq!(
            values,
            vec![
                serde_json::to_string(&config).expect("encode"),
                "Bearer abc-def-ghi".to_string(),
                "abc-def".to_string(),
            ]
        );
    }

    #[test]
    fn an_unrestorable_server_reports_its_settings_with_only_the_values_masked() {
        let config = config(serde_json::json!({
            "command": "npx",
            "args": ["-y", "@upstash/context7-mcp"],
            "env": {"API_KEY": "sk-secret", "REGION": "eu-west-1"}
        }));
        let error = UiError::from(CoreError::UnrestorableMcpServer {
            name: "context7".to_string(),
            config_json: serde_json::to_string(&config).expect("encode"),
        });

        assert!(!error.message.contains("sk-secret"));
        assert!(!error.message.contains("eu-west-1"));
        assert!(error.message.contains("context7"));
        assert!(error.message.contains("npx"));
        assert!(error.message.contains("@upstash/context7-mcp"));
        assert!(error.message.contains("API_KEY"));
        assert!(error.message.contains("REGION"));
        assert!(error.message.contains(MASK));
    }

    #[test]
    fn an_unrestorable_config_that_cannot_be_rebuilt_falls_back_to_a_bare_mask() {
        let error = UiError::from(CoreError::UnrestorableMcpServer {
            name: "context7".to_string(),
            config_json: "{not json".to_string(),
        });

        assert!(!error.message.contains("not json"));
        assert!(error.message.contains(MASK));
    }

    #[test]
    fn a_value_too_short_to_be_a_secret_is_never_a_needle() {
        let config = config(serde_json::json!({"env": {"PORT": "30", "COUNT": "1"}}));
        let values = secret_values(std::iter::once(&config));

        assert!(!values.iter().any(|value| value == "30" || value == "1"));
        assert_eq!(
            redact("listening on 30, retried 1 time".to_string(), &values),
            "listening on 30, retried 1 time"
        );
    }

    #[test]
    fn a_secret_is_gathered_in_the_escaped_form_the_cli_echoes_back() {
        let config = config(serde_json::json!({
            "env": {"KEY": "-----BEGIN KEY-----\nline \"one\"\\two\n-----END KEY-----"}
        }));
        let values = secret_values(std::iter::once(&config));
        let encoded = serde_json::to_string(&config).expect("encode");

        assert!(values.contains(&encoded));
        assert!(values.iter().any(|value| value.contains("\\n")));
        assert!(values.iter().any(|value| value.contains("\\\"one\\\"")));
    }

    #[test]
    fn the_whole_echoed_config_is_scrubbed_however_the_cli_quotes_it() {
        let secret = "p@ss\"word\\with\nbreaks";
        let config = config(serde_json::json!({
            "command": "npx",
            "env": {"TOKEN": secret}
        }));
        let encoded = serde_json::to_string(&config).expect("encode");
        let values = secret_values(std::iter::once(&config));

        let echoed = format!("Invalid server config: {encoded}\n(rejected)");
        let redacted = redact(echoed, &values);

        assert!(!redacted.contains(&encoded));
        assert!(!redacted.contains(secret));
        assert!(!redacted.contains("p@ss"));
        assert!(!redacted.contains("word"));
        assert!(redacted.starts_with(&format!("Invalid server config: {MASK}")));
    }

    #[test]
    fn redact_replaces_every_occurrence_of_a_value() {
        let redacted = redact("a=secret b=secret".to_string(), &["secret".to_string()]);
        assert_eq!(redacted, format!("a={MASK} b={MASK}"));
    }

    /// A caller that assembles its own needles must not be able to under-mask by listing the short
    /// one first, so the ordering is redact's own concern rather than the caller's.
    #[test]
    fn redact_masks_the_longest_match_whatever_order_the_needles_arrive_in() {
        let redacted = redact(
            "LONG=abc-def-ghi".to_string(),
            &["abc".to_string(), "abc-def-ghi".to_string()],
        );
        assert_eq!(redacted, format!("LONG={MASK}"));
    }

    #[test]
    fn an_unrestorable_server_masks_its_header_values_too() {
        let config = config(serde_json::json!({
            "kind": "http",
            "url": "https://example.com/mcp",
            "headers": {"Authorization": "Bearer sk-secret"}
        }));
        let error = UiError::from(CoreError::UnrestorableMcpServer {
            name: "figma".to_string(),
            config_json: serde_json::to_string(&config).expect("encode"),
        });

        assert!(!error.message.contains("sk-secret"));
        assert!(error.message.contains("https://example.com/mcp"));
        assert!(error.message.contains("Authorization"));
        assert!(error.message.contains(MASK));
    }

    #[test]
    fn a_failed_cli_command_puts_its_stderr_in_the_detail() {
        let error = UiError::from(CoreError::Cli {
            command: "claude plugin list".to_string(),
            stderr: "not logged in".to_string(),
        });
        assert!(error.message.contains("claude plugin list"));
        assert_eq!(error.detail.as_deref(), Some("not logged in"));
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn sensitive_names_follow_their_last_segment() {
        for name in [
            "--api-key",
            "--apiKey",
            "--apikey",
            "APIKey",
            "GITHUB_TOKEN",
            "--auth-token",
            "--authToken",
            "--clientSecret",
            "--secret-key",
            "--private-key",
            "x-api-key",
            "-password",
            "Authorization",
            "--auth",
            "db.pass",
            "--access_token",
        ] {
            assert!(is_sensitive_name(name), "{name}");
        }
        for name in [
            "--no-auth",
            "--auth-type",
            "--key-file",
            "--token-file",
            "--keyboard",
            "--port",
            "--pass-through",
            "--passthrough",
            "REGION",
            "-y",
            "-e",
            "",
        ] {
            assert!(!is_sensitive_name(name), "{name}");
        }
    }

    #[test]
    fn a_boolean_auth_flag_leaves_the_next_argument_alone() {
        let args = strings(&["--no-auth", "server.js", "--auth-type", "oauth"]);
        assert_eq!(mask_args(&args), args);
    }

    #[test]
    fn values_are_inspected_when_the_name_is_not_sensitive() {
        let args = strings(&[
            "DATABASE_URL=postgresql://u:hunter22@h/db",
            "--url=https://h/mcp?api_key=x1234567",
            "--header=Authorization: Bearer x1234567",
            "Authorization:Bearer sk-x",
            "Authorization: Bearer sk-y",
        ]);
        assert_eq!(
            mask_args(&args),
            vec![
                format!("DATABASE_URL=postgresql://u:{MASK}@h/db"),
                format!("--url=https://h/mcp?api_key={MASK}"),
                format!("--header=Authorization: {MASK}"),
                format!("Authorization:{MASK}"),
                format!("Authorization: {MASK}"),
            ]
        );
        let secrets: Vec<String> = arg_secrets(&args)
            .into_iter()
            .map(|secret| secret.value)
            .collect();
        for secret in ["hunter22", "x1234567", "Bearer x1234567", "Bearer sk-x"] {
            assert!(secrets.contains(&secret.to_string()), "{secret}");
        }
    }

    #[test]
    fn args_mask_every_secret_shape() {
        let args = strings(&[
            "--api-key=sk-123456",
            "--token",
            "sk-abcdef",
            "-e",
            "GITHUB_TOKEN=ghp_xyz789",
            "Authorization: Bearer abc123",
            "--auth=",
        ]);
        assert_eq!(
            mask_args(&args),
            vec![
                format!("--api-key={MASK}"),
                "--token".to_string(),
                MASK.to_string(),
                "-e".to_string(),
                format!("GITHUB_TOKEN={MASK}"),
                format!("Authorization: {MASK}"),
                "--auth=".to_string(),
            ]
        );
    }

    #[test]
    fn args_without_secrets_stay_as_they_are() {
        let args = strings(&[
            "-y",
            "@scope/pkg@1.2.3",
            "--port",
            "3000",
            "--keyboard",
            "x",
            "--region=eu",
            "REGION=eu",
            "--token",
            "--verbose",
        ]);
        assert_eq!(mask_args(&args), args);
    }

    #[test]
    fn urls_in_args_are_masked_like_urls() {
        let args = strings(&["https://host/mcp?apiKey=abcd1234"]);
        assert_eq!(
            mask_args(&args),
            vec![format!("https://host/mcp?apiKey={MASK}")]
        );
    }

    #[test]
    fn url_userinfo_password_and_every_query_value_are_masked() {
        let (masked, secrets) = mask_url("https://user:pw1234@host:8080/mcp?a=one&b=two&flag#frag");
        assert_eq!(
            masked,
            format!("https://user:{MASK}@host:8080/mcp?a={MASK}&b={MASK}&flag#frag")
        );
        assert_eq!(secrets, strings(&["pw1234", "one", "two"]));
    }

    #[test]
    fn a_url_with_no_secrets_is_byte_identical() {
        for url in [
            "https://example.com/mcp",
            "https://user@example.com/mcp#top",
            "https://example.com/mcp?flag&x=",
            "not a url",
        ] {
            let (masked, secrets) = mask_url(url);
            assert_eq!(masked, url);
            assert!(secrets.is_empty());
        }
    }

    #[test]
    fn an_unparsable_url_still_loses_what_follows_its_question_mark() {
        let (masked, _) = mask_url("example.com/mcp?key=abcd");
        assert_eq!(masked, format!("example.com/mcp?key={MASK}"));
    }

    #[test]
    fn the_ui_config_lists_which_args_and_whether_the_url_are_masked() {
        let ui = UiMcpConfig::from(&config(serde_json::json!({
            "command": "npx",
            "args": ["-y", "pkg", "--token", "sk-secret", "--api-key=sk-other"],
            "url": "https://host/mcp?apiKey=sk-url"
        })));
        assert_eq!(ui.masked_args, vec![3, 4]);
        assert!(ui.url_masked);
        let json = serde_json::to_string(&ui).expect("serialises");
        assert!(!json.contains("sk-secret"));
        assert!(!json.contains("sk-other"));
        assert!(!json.contains("sk-url"));

        let plain = UiMcpConfig::from(&config(serde_json::json!({
            "args": ["-y", "pkg"], "url": "https://host/mcp"
        })));
        assert!(plain.masked_args.is_empty());
        assert!(!plain.url_masked);
    }

    #[test]
    fn reveal_returns_the_original_arg_and_url() {
        let full = config(serde_json::json!({
            "args": ["--token", "sk-secret"],
            "url": "https://host/mcp?apiKey=sk-url"
        }));
        let url_mask = format!("https://host/mcp?apiKey={MASK}");
        assert_eq!(
            reveal_field("s", &full, UiSecretField::Args, "1", Some(MASK)).expect("arg"),
            "sk-secret"
        );
        assert_eq!(
            reveal_field("s", &full, UiSecretField::Url, "", Some(&url_mask)).expect("url"),
            "https://host/mcp?apiKey=sk-url"
        );
        for key in ["2", "-1", "x", ""] {
            assert!(
                reveal_field("s", &full, UiSecretField::Args, key, Some(MASK)).is_err(),
                "{key}"
            );
        }
        let bare = config(serde_json::json!({"command": "npx"}));
        assert!(reveal_field("s", &bare, UiSecretField::Url, "", Some("")).is_err());
        assert!(reveal_field("s", &bare, UiSecretField::Args, "0", Some("")).is_err());
    }

    #[test]
    fn reveal_refuses_an_argument_or_url_that_is_not_the_one_shown() {
        let shifted = config(serde_json::json!({
            "args": ["--verbose", "--token", "sk-secret"],
            "url": "https://host/mcp?apiKey=sk-url"
        }));
        let error = reveal_field("s", &shifted, UiSecretField::Args, "1", Some(MASK))
            .expect_err("index 1 is now a flag");
        assert!(error.message.contains("changed since it was shown"));
        assert!(reveal_field("s", &shifted, UiSecretField::Args, "2", None).is_err());
        assert!(reveal_field("s", &shifted, UiSecretField::Args, "2", Some("--other")).is_err());
        assert!(reveal_field("s", &shifted, UiSecretField::Url, "", None).is_err());
        assert!(
            reveal_field(
                "s",
                &shifted,
                UiSecretField::Url,
                "",
                Some("https://host/mcp?apiKey=sk-url")
            )
            .is_err()
        );
    }

    #[test]
    fn short_secrets_behind_sensitive_names_are_still_scrubbed_from_cli_output() {
        let config = config(serde_json::json!({
            "args": ["--password", "hunter2", "--token=abc1234"],
            "url": "postgres://u:secret@h/db"
        }));
        let values = secret_values(std::iter::once(&config));
        let block =
            "srv:\n  Args: --password hunter2 --token=abc1234\n  URL: postgres://u:secret@h/db\n";
        let redacted = redact(block.to_string(), &values);
        for leaked in ["hunter2", "abc1234", "secret"] {
            assert!(!redacted.contains(leaked), "{leaked}");
        }
    }

    #[test]
    fn short_query_values_are_not_redaction_needles() {
        let config =
            config(serde_json::json!({"url": "https://host/mcp?format=json&k=longenough1"}));
        let values = secret_values(std::iter::once(&config));
        assert!(!values.contains(&"json".to_string()));
        assert!(values.contains(&"longenough1".to_string()));
        assert_eq!(
            redact("see .mcp.json".to_string(), &values),
            "see .mcp.json"
        );
    }

    #[test]
    fn secret_values_cover_arg_and_url_secrets_and_redact_scrubs_a_mcp_get_block() {
        let config = config(serde_json::json!({
            "command": "npx",
            "args": ["--token", "sk-argsecret", "--api-key=sk-eqsecret"],
            "url": "https://user:pw-secret@host/mcp?apiKey=sk-urlsecret"
        }));
        let values = secret_values(std::iter::once(&config));
        for secret in ["sk-argsecret", "sk-eqsecret", "pw-secret", "sk-urlsecret"] {
            assert!(values.contains(&secret.to_string()), "{secret}");
        }
        let block = "my-server:\n  Args: --token sk-argsecret --api-key=sk-eqsecret\n  URL: https://user:pw-secret@host/mcp?apiKey=sk-urlsecret\n";
        let redacted = redact(block.to_string(), &values);
        for secret in ["sk-argsecret", "sk-eqsecret", "pw-secret", "sk-urlsecret"] {
            assert!(!redacted.contains(secret), "{secret}");
        }
        assert!(redacted.contains(MASK));
    }

    #[test]
    fn to_config_refuses_a_masked_arg_or_url() {
        let input = |args: Option<Vec<String>>, url: Option<&str>| UiMcpConfigInput {
            kind: None,
            command: Some("npx".to_string()),
            args,
            url: url.map(str::to_string),
            env: None,
            headers: None,
        };
        let error = input(Some(strings(&["-y", &format!("--token={MASK}")])), None)
            .to_config()
            .expect_err("masked arg");
        assert!(error.message.contains("Argument 2"));
        assert!(error.message.contains("still hidden"));
        let error = input(None, Some(&format!("https://host/?a={MASK}")))
            .to_config()
            .expect_err("masked url");
        assert!(error.message.contains("The URL"));
        assert!(
            input(Some(strings(&["-y"])), Some("https://host/"))
                .to_config()
                .is_ok()
        );
    }

    #[test]
    fn the_unrestorable_message_masks_args_and_url() {
        let json =
            r#"{"command":"npx","args":["--token","sk-argsecret"],"url":"https://host/?k=sk-url"}"#;
        let masked = value_masked_config(json);
        assert!(!masked.contains("sk-argsecret"));
        assert!(!masked.contains("sk-url"));
        assert!(masked.contains("--token"));
    }
}
