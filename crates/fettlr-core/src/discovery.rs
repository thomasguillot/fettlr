use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::cli_records::{CoreError, McpServerConfig, PluginRecord};
use crate::frontmatter::parse_frontmatter;
use crate::models::Scope;
use crate::paths::Paths;
use crate::skills::{Provenance, Skill};
use crate::subagents::Subagent;
use crate::usage::UsageIndex;

#[derive(Clone, Debug, Serialize)]
pub struct ProjectEntry {
    pub path: String,
    pub exists: bool,
    pub has_trust_dialog_accepted: bool,
    pub local_mcp_servers: BTreeMap<String, McpServerConfig>,
    pub enabled_mcpjson_servers: Vec<String>,
    pub disabled_mcpjson_servers: Vec<String>,
    pub disabled_mcp_servers: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DiscoveredConfig {
    pub user_mcp_servers: BTreeMap<String, McpServerConfig>,
    pub projects: Vec<ProjectEntry>,
    pub user_skills: Vec<Skill>,
    pub project_skills: Vec<Skill>,
    pub user_subagents: Vec<Subagent>,
    pub project_subagents: Vec<Subagent>,
    pub project_mcp_json: BTreeMap<String, BTreeMap<String, McpServerConfig>>,
    pub usage: UsageIndex,
    pub warnings: Vec<String>,
    /// When the read started, on a monotonic clock: a wall-clock stamp can be moved past a later
    /// mutation by a clock adjustment, which would make a stale snapshot look trustworthy.
    pub read_at: Instant,
}

impl Default for DiscoveredConfig {
    fn default() -> Self {
        Self {
            user_mcp_servers: BTreeMap::new(),
            projects: Vec::new(),
            user_skills: Vec::new(),
            project_skills: Vec::new(),
            user_subagents: Vec::new(),
            project_subagents: Vec::new(),
            project_mcp_json: BTreeMap::new(),
            usage: UsageIndex::default(),
            warnings: Vec::new(),
            read_at: Instant::now(),
        }
    }
}

/// The connectors Claude Code has connected to on this account. Its own record, and the only local
/// trace of a connector that is configured but not currently being served.
pub fn ever_connected_connectors(paths: &Paths) -> Vec<String> {
    let Ok(root) = read_root_config(&paths.claude_json) else {
        return Vec::new();
    };
    string_array(root.get("claudeAiMcpEverConnected"))
}

pub fn discover(paths: &Paths) -> Result<DiscoveredConfig, CoreError> {
    let mut warnings = Vec::new();
    let read_at = Instant::now();
    let root = read_root_config(&paths.claude_json)?;
    let location = paths.claude_json.display().to_string();

    let user_mcp_servers = servers_from(root.get("mcpServers"), &location, &mut warnings);
    let projects = project_entries(root.get("projects"), &location, &mut warnings);
    let usage = UsageIndex::from_root(&root);
    let user_skills = skills_in(&paths.skills_dir(), &Scope::User, true, &mut warnings);
    let user_subagents = subagents_in(&paths.agents_dir(), &Scope::User, &mut warnings);

    let mut project_skills = Vec::new();
    let mut project_subagents = Vec::new();
    let mut project_mcp_json = BTreeMap::new();
    for project in &projects {
        if !project.exists {
            continue;
        }
        let directory = PathBuf::from(&project.path);
        let project_skills_dir = directory.join(".claude").join("skills");
        let duplicates_user_skills = project_skills_dir == paths.skills_dir();
        if !duplicates_user_skills {
            project_skills.extend(skills_in(
                &project_skills_dir,
                &Scope::Project {
                    path: project.path.clone(),
                },
                project.has_trust_dialog_accepted,
                &mut warnings,
            ));
        }

        let project_agents_dir = directory.join(".claude").join("agents");
        if project_agents_dir != paths.agents_dir() {
            project_subagents.extend(subagents_in(
                &project_agents_dir,
                &Scope::Project {
                    path: project.path.clone(),
                },
                &mut warnings,
            ));
        }

        let mcp_json_path = directory.join(".mcp.json");
        if let Some(servers) = mcp_json_servers(&mcp_json_path, &mut warnings) {
            project_mcp_json.insert(project.path.clone(), servers);
        }
    }

    Ok(DiscoveredConfig {
        user_mcp_servers,
        projects,
        user_skills,
        project_skills,
        user_subagents,
        project_subagents,
        project_mcp_json,
        usage,
        warnings,
        read_at,
    })
}

fn read_root_config(path: &Path) -> Result<Map<String, Value>, CoreError> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = std::fs::read_to_string(path).map_err(|_| CoreError::UnreadableRootConfig {
        path: path.display().to_string(),
    })?;
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(CoreError::MalformedRootConfig {
            path: path.display().to_string(),
        }),
    }
}

fn servers_from(
    raw: Option<&Value>,
    location: &str,
    warnings: &mut Vec<String>,
) -> BTreeMap<String, McpServerConfig> {
    let Some(raw) = raw else {
        return BTreeMap::new();
    };
    let Some(entries) = raw.as_object() else {
        warnings.push(format!(
            "Could not read the MCP servers listed in {location}."
        ));
        return BTreeMap::new();
    };

    let mut result = BTreeMap::new();
    for (name, value) in entries {
        match serde_json::from_value::<McpServerConfig>(value.clone()) {
            Ok(config) => {
                result.insert(name.clone(), config);
            }
            Err(_) => warnings.push(format!(
                "Skipped the MCP server {name} in {location} because its settings could not be read."
            )),
        }
    }
    result
}

fn project_entries(
    raw: Option<&Value>,
    location: &str,
    warnings: &mut Vec<String>,
) -> Vec<ProjectEntry> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let Some(entries) = raw.as_object() else {
        warnings.push(format!(
            "Could not read the list of projects in {location}."
        ));
        return Vec::new();
    };

    let mut result = Vec::new();
    for (path, value) in entries {
        let Some(object) = value.as_object() else {
            warnings.push(format!(
                "Skipped the project {path} because its entry in {location} could not be read."
            ));
            continue;
        };
        result.push(ProjectEntry {
            path: path.clone(),
            exists: Path::new(path).is_dir(),
            has_trust_dialog_accepted: object
                .get("hasTrustDialogAccepted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            local_mcp_servers: servers_from(object.get("mcpServers"), path, warnings),
            enabled_mcpjson_servers: string_array(object.get("enabledMcpjsonServers")),
            disabled_mcpjson_servers: string_array(object.get("disabledMcpjsonServers")),
            disabled_mcp_servers: string_array(object.get("disabledMcpServers")),
        });
    }
    let accepted: std::collections::HashSet<PathBuf> = result
        .iter()
        .filter(|entry| entry.has_trust_dialog_accepted)
        .map(|entry| PathBuf::from(&entry.path))
        .collect();
    for entry in &mut result {
        entry.has_trust_dialog_accepted = Path::new(&entry.path)
            .ancestors()
            .any(|ancestor| accepted.contains(ancestor));
    }
    result
}

pub(crate) fn string_array(raw: Option<&Value>) -> Vec<String> {
    raw.and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn mcp_json_servers(
    path: &Path,
    warnings: &mut Vec<String>,
) -> Option<BTreeMap<String, McpServerConfig>> {
    if !path.exists() {
        return None;
    }
    let location = path.display().to_string();
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => {
            warnings.push(format!("Could not read {location}."));
            return None;
        }
    };
    let object: Map<String, Value> = match serde_json::from_str(&text) {
        Ok(object) => object,
        Err(_) => {
            warnings.push(format!("Could not read {location}."));
            return None;
        }
    };
    Some(servers_from(object.get("mcpServers"), &location, warnings))
}

/// The skills a plugin ships, read from each installed plugin's own `installPath/skills`.
/// Read-only: they are managed through the plugin, never moved or deleted by Fettlr. Keyed by
/// plugin id rather than scope, so the same plugin installed in two scopes lists its skills once.
pub fn plugin_skills(plugins: &[PluginRecord]) -> (Vec<Skill>, Vec<String>) {
    const PLUGIN_SKILL_DEPTH: u8 = 3;

    let mut warnings = Vec::new();
    let mut found: BTreeMap<String, Skill> = BTreeMap::new();
    for plugin in plugins {
        if plugin.install_path.is_empty() {
            continue;
        }
        let directory = PathBuf::from(&plugin.install_path).join("skills");
        for skill in plugin_skills_in(&directory, plugin, PLUGIN_SKILL_DEPTH, &mut warnings) {
            found.entry(skill.id.clone()).or_insert(skill);
        }
    }

    (found.into_values().collect(), warnings)
}

fn plugin_skills_in(
    directory: &Path,
    plugin: &PluginRecord,
    depth: u8,
    warnings: &mut Vec<String>,
) -> Vec<Skill> {
    if depth == 0 || !directory.is_dir() {
        return Vec::new();
    }
    let Some(names) = sorted_directory_names(directory, warnings) else {
        return Vec::new();
    };

    let mut result = Vec::new();
    for name in names {
        let folder = directory.join(&name);
        if !folder.is_dir() {
            continue;
        }
        // A plugin may group its skills in subfolders, and may keep folders that are not skills
        // at all, so only a folder carrying its own SKILL.md counts as one.
        if folder.join("SKILL.md").is_file() {
            result.push(plugin_skill_at(&folder, &name, plugin, warnings));
        } else {
            result.extend(plugin_skills_in(&folder, plugin, depth - 1, warnings));
        }
    }
    result
}

fn sorted_directory_names(directory: &Path, warnings: &mut Vec<String>) -> Option<Vec<String>> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => {
            warnings.push(format!(
                "Could not read the folder at {}.",
                directory.display()
            ));
            return None;
        }
    };

    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    Some(names)
}

fn skills_in(
    directory: &Path,
    scope: &Scope,
    trusted: bool,
    warnings: &mut Vec<String>,
) -> Vec<Skill> {
    if !directory.is_dir() {
        return Vec::new();
    }
    let Some(names) = sorted_directory_names(directory, warnings) else {
        return Vec::new();
    };

    names
        .into_iter()
        .filter_map(|name| {
            let folder = directory.join(&name);
            folder
                .is_dir()
                .then(|| skill_at(&folder, &name, scope, trusted, warnings))
        })
        .collect()
}

fn skill_frontmatter_fields(folder: &Path, warnings: &mut Vec<String>) -> BTreeMap<String, String> {
    let markdown_path = folder.join("SKILL.md");
    let mut fields = BTreeMap::new();
    if markdown_path.is_file() {
        match std::fs::read_to_string(&markdown_path) {
            Ok(text) => fields = parse_frontmatter(&text),
            Err(_) => warnings.push(format!("Could not read {}.", markdown_path.display())),
        }
    }
    fields
}

fn skill_at(
    folder: &Path,
    name: &str,
    scope: &Scope,
    trusted: bool,
    warnings: &mut Vec<String>,
) -> Skill {
    let fields = skill_frontmatter_fields(folder, warnings);
    Skill {
        id: format!("{}/{}", scope.key(), name),
        name: name.to_string(),
        title: non_empty(fields.get("name")),
        summary: non_empty(fields.get("description")),
        scope: scope.clone(),
        provenance: provenance_of(folder, trusted),
        path: folder.to_path_buf(),
    }
}

fn plugin_skill_at(
    folder: &Path,
    name: &str,
    plugin: &PluginRecord,
    warnings: &mut Vec<String>,
) -> Skill {
    let fields = skill_frontmatter_fields(folder, warnings);
    Skill {
        id: format!("plugin:{}/{}", plugin.plugin_id, name),
        name: name.to_string(),
        title: non_empty(fields.get("name")),
        summary: non_empty(fields.get("description")),
        scope: plugin.scope.clone(),
        provenance: Provenance::Plugin {
            plugin_id: plugin.plugin_id.clone(),
        },
        path: folder.to_path_buf(),
    }
}

/// The subagents a plugin ships. Read-only, like its skills: managed through the plugin, never
/// moved or trashed by Fettlr. The manifest's `agents` field, when present, replaces the default
/// `agents/` folder rather than adding to it, which is what Claude Code does.
pub fn plugin_subagents(plugins: &[PluginRecord]) -> (Vec<Subagent>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut found: BTreeMap<String, Subagent> = BTreeMap::new();
    for plugin in plugins {
        if plugin.install_path.is_empty() {
            continue;
        }
        let install = PathBuf::from(&plugin.install_path);
        for root in plugin_agent_roots(&install, &mut warnings) {
            let mut emit = |relative: &str, file: &Path, fields: BTreeMap<String, String>| {
                let subagent = Subagent::from_fields(
                    &fields,
                    format!("plugin:{}::{}", plugin.plugin_id, relative),
                    format!("{relative}.md"),
                    stem_of(file),
                    plugin.scope.clone(),
                    Provenance::Plugin {
                        plugin_id: plugin.plugin_id.clone(),
                    },
                    file.to_path_buf(),
                );
                found.entry(subagent.id.clone()).or_insert(subagent);
            };
            if root.is_file() {
                if root.extension().and_then(|e| e.to_str()) == Some("md") {
                    let fields = subagent_frontmatter_fields(&root, &mut warnings);
                    emit(stem_of(&root), &root, fields);
                }
            } else {
                collect_subagents(&root, "", SUBAGENT_DEPTH, &mut warnings, &mut emit);
            }
        }
    }
    (found.into_values().collect(), warnings)
}

fn plugin_agent_roots(install: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let manifest = install.join(".claude-plugin").join("plugin.json");
    let Ok(text) = std::fs::read_to_string(&manifest) else {
        return vec![install.join("agents")];
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            warnings.push(format!("Could not parse {}.", manifest.display()));
            return vec![install.join("agents")];
        }
    };
    let listed: Vec<String> = match value.get("agents") {
        Some(serde_json::Value::String(one)) => vec![one.clone()],
        Some(serde_json::Value::Array(many)) => many
            .iter()
            .filter_map(|entry| entry.as_str().map(str::to_string))
            .collect(),
        _ => return vec![install.join("agents")],
    };
    listed
        .iter()
        .filter_map(|entry| {
            let trimmed = entry.trim_start_matches("./");
            if trimmed.is_empty() || trimmed == "." {
                warnings.push(format!(
                    "Ignored the agents path {entry:?} in {}: it names the whole plugin.",
                    manifest.display()
                ));
                return None;
            }
            let candidate = Path::new(entry);
            if candidate.is_absolute()
                || candidate
                    .components()
                    .any(|component| component == std::path::Component::ParentDir)
            {
                warnings.push(format!(
                    "Ignored the agents path {entry} in {}: it points outside the plugin.",
                    manifest.display()
                ));
                return None;
            }
            Some(install.join(trimmed))
        })
        .collect()
}

fn provenance_of(folder: &Path, trusted: bool) -> Provenance {
    if trusted && folder.join(".git").exists() {
        Provenance::GitClone
    } else {
        Provenance::Local
    }
}

fn non_empty(value: Option<&String>) -> Option<String> {
    value.filter(|value| !value.is_empty()).cloned()
}

const SUBAGENT_DEPTH: u8 = 3;

type SubagentSink<'a> = dyn FnMut(&str, &Path, BTreeMap<String, String>) + 'a;

fn subagents_in(directory: &Path, scope: &Scope, warnings: &mut Vec<String>) -> Vec<Subagent> {
    let mut found = Vec::new();
    collect_subagents(
        directory,
        "",
        SUBAGENT_DEPTH,
        warnings,
        &mut |relative, file, fields| {
            let id = format!("{}::{}", scope.key(), relative);
            found.push(Subagent::from_fields(
                &fields,
                id,
                format!("{relative}.md"),
                stem_of(file),
                scope.clone(),
                Provenance::Local,
                file.to_path_buf(),
            ));
        },
    );
    found
}

fn collect_subagents(
    directory: &Path,
    prefix: &str,
    depth: u8,
    warnings: &mut Vec<String>,
    emit: &mut SubagentSink<'_>,
) {
    if depth == 0 || !directory.is_dir() {
        return;
    }
    let Some(names) = sorted_directory_names(directory, warnings) else {
        return;
    };
    for name in names {
        let entry = directory.join(&name);
        if entry.is_dir() {
            let nested = format!("{prefix}{name}/");
            collect_subagents(&entry, &nested, depth - 1, warnings, emit);
            continue;
        }
        if !entry.is_file() || Path::new(&name).extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let relative = format!("{prefix}{}", stem_of(&entry));
        emit(
            &relative,
            &entry,
            subagent_frontmatter_fields(&entry, warnings),
        );
    }
}

fn subagent_frontmatter_fields(
    file: &Path,
    warnings: &mut Vec<String>,
) -> BTreeMap<String, String> {
    match std::fs::read_to_string(file) {
        Ok(text) => parse_frontmatter(&text),
        Err(_) => {
            warnings.push(format!("Could not read {}.", file.display()));
            BTreeMap::new()
        }
    }
}

fn stem_of(file: &Path) -> &str {
    file.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
}
