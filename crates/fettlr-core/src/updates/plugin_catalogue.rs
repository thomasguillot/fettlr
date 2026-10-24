use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::Deserialize;
use serde_json::Value;

use crate::paths::Paths;

use super::checker::read_gcs_sha;

/// What the marketplaces on disk say is the current version of a plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogueVersion {
    Version(String),
    Commit(String),
    Untracked,
}

/// Latest versions keyed by `<plugin>@<marketplace>`, read from the same local marketplace
/// clones `claude plugin update` consults.
#[derive(Clone, Debug, Default)]
pub struct PluginCatalogue {
    entries: BTreeMap<String, CatalogueVersion>,
    installed_commits: BTreeMap<String, String>,
}

impl PluginCatalogue {
    /// The commit Claude Code recorded for an installed plugin, keyed like `PluginRecord::id`.
    pub fn installed_commit(&self, record_id: &str) -> Option<&str> {
        self.installed_commits.get(record_id).map(String::as_str)
    }

    pub fn get(&self, plugin_id: &str) -> Option<&CatalogueVersion> {
        self.entries.get(plugin_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl FromIterator<(String, CatalogueVersion)> for PluginCatalogue {
    fn from_iter<I: IntoIterator<Item = (String, CatalogueVersion)>>(iter: I) -> Self {
        Self {
            entries: iter.into_iter().collect(),
            installed_commits: BTreeMap::new(),
        }
    }
}

#[derive(Deserialize)]
struct KnownMarketplace {
    #[serde(default)]
    source: Value,
    #[serde(default, rename = "installLocation")]
    install_location: Option<String>,
}

#[derive(Deserialize)]
struct MarketplaceManifest {
    #[serde(default)]
    plugins: Vec<Value>,
}

#[derive(Deserialize)]
struct MarketplaceEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    source: Value,
}

#[derive(Deserialize)]
struct PluginManifest {
    #[serde(default)]
    version: Option<String>,
}

/// Never fails: anything missing or malformed contributes nothing and is logged.
pub fn read_plugin_catalogue(paths: &Paths) -> PluginCatalogue {
    let known_path = paths.known_marketplaces_json();
    let Some(known) = read_json::<BTreeMap<String, Value>>(&known_path) else {
        return PluginCatalogue::default();
    };
    let mut entries = BTreeMap::new();
    for (marketplace, record) in known {
        let record = match serde_json::from_value::<KnownMarketplace>(record) {
            Ok(record) => record,
            Err(error) => {
                log::warn!(
                    "skipping marketplace {marketplace} in {}: {error}",
                    known_path.display()
                );
                continue;
            }
        };
        let dir = match &record.install_location {
            Some(location) if !location.is_empty() => Path::new(location).to_path_buf(),
            _ => paths.marketplaces_dir().join(&marketplace),
        };
        let manifest_path = dir.join(".claude-plugin").join("marketplace.json");
        let Some(manifest) = read_json::<MarketplaceManifest>(&manifest_path) else {
            continue;
        };
        let git_hosted = matches!(
            record.source.get("source").and_then(Value::as_str),
            Some("github" | "git")
        );
        let downloaded_commit = if git_hosted && !dir.join(".git").exists() {
            read_gcs_sha(&dir)
        } else {
            None
        };
        for raw in manifest.plugins {
            let entry = match serde_json::from_value::<MarketplaceEntry>(raw.clone()) {
                Ok(entry) => entry,
                Err(error) => {
                    let name = raw.get("name").and_then(Value::as_str).unwrap_or("?");
                    log::warn!(
                        "skipping plugin {name} in {}: {error}",
                        manifest_path.display()
                    );
                    continue;
                }
            };
            let id = format!("{}@{marketplace}", entry.name);
            if entry.name.is_empty() || entries.contains_key(&id) {
                continue;
            }
            entries.insert(
                id,
                classify(&entry, &dir, git_hosted, downloaded_commit.as_deref()),
            );
        }
    }
    PluginCatalogue {
        entries,
        installed_commits: read_installed_commits(paths),
    }
}

#[derive(Deserialize)]
struct InstalledPlugins {
    #[serde(default)]
    plugins: BTreeMap<String, Vec<Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledPlugin {
    scope: String,
    #[serde(default)]
    project_path: Option<String>,
    #[serde(default)]
    git_commit_sha: Option<String>,
}

fn read_installed_commits(paths: &Paths) -> BTreeMap<String, String> {
    let mut commits = BTreeMap::new();
    if !paths.installed_plugins_json().exists() {
        return commits;
    }
    let Some(installed) = read_json::<InstalledPlugins>(&paths.installed_plugins_json()) else {
        return commits;
    };
    for (plugin_id, installs) in installed.plugins {
        for raw in installs {
            let Ok(install) = serde_json::from_value::<InstalledPlugin>(raw) else {
                continue;
            };
            let Some(sha) = install.git_commit_sha.filter(|sha| is_full_sha(sha)) else {
                continue;
            };
            let scope = match (install.scope.as_str(), install.project_path) {
                ("user", _) => "user".to_string(),
                ("project", Some(path)) => format!("project:{path}"),
                ("local", Some(path)) => format!("local:{path}"),
                _ => continue,
            };
            commits.insert(format!("{plugin_id}#{scope}"), sha.to_ascii_lowercase());
        }
    }
    commits
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            log::warn!("cannot read {}: {error}", path.display());
            return None;
        }
    };
    match serde_json::from_str(&raw) {
        Ok(value) => Some(value),
        Err(error) => {
            log::warn!("cannot parse {}: {error}", path.display());
            None
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|v| !v.is_empty()).map(str::to_string)
}

fn classify(
    entry: &MarketplaceEntry,
    marketplace_dir: &Path,
    git_hosted: bool,
    downloaded_commit: Option<&str>,
) -> CatalogueVersion {
    let entry_version = non_empty(entry.version.as_deref());
    match &entry.source {
        Value::String(relative) => {
            if !git_hosted || !stays_inside(relative) {
                return CatalogueVersion::Untracked;
            }
            let manifest_path = marketplace_dir
                .join(relative)
                .join(".claude-plugin")
                .join("plugin.json");
            let manifest_version = std::fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|raw| match serde_json::from_str::<PluginManifest>(&raw) {
                    Ok(manifest) => Some(manifest),
                    Err(error) => {
                        log::warn!("cannot parse {}: {error}", manifest_path.display());
                        None
                    }
                })
                .and_then(|manifest| non_empty(manifest.version.as_deref()));
            match manifest_version.or(entry_version) {
                Some(version) => CatalogueVersion::Version(version),
                None => downloaded_commit.map_or(CatalogueVersion::Untracked, |sha| {
                    CatalogueVersion::Commit(sha.to_string())
                }),
            }
        }
        Value::Object(source) => {
            let kind = source.get("source").and_then(Value::as_str);
            match kind {
                Some("github" | "url" | "git-subdir") => {
                    if let Some(version) = entry_version {
                        return CatalogueVersion::Version(version);
                    }
                    let sha = source.get("sha").and_then(Value::as_str);
                    match (kind, sha) {
                        (Some("github" | "url"), Some(sha)) if is_full_sha(sha) => {
                            CatalogueVersion::Commit(sha.to_string())
                        }
                        _ => CatalogueVersion::Untracked,
                    }
                }
                _ => CatalogueVersion::Untracked,
            }
        }
        _ => CatalogueVersion::Untracked,
    }
}

fn stays_inside(relative: &str) -> bool {
    let path = Path::new(relative);
    !path.is_absolute() && !path.has_root() && !path.components().any(|c| c == Component::ParentDir)
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}
