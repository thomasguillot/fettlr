use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::Deserialize;
use serde_json::Value;

use crate::paths::Paths;

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
}

impl PluginCatalogue {
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
    plugins: Vec<MarketplaceEntry>,
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
    let Some(known) = read_json::<BTreeMap<String, KnownMarketplace>>(&known_path) else {
        return PluginCatalogue::default();
    };
    let mut entries = BTreeMap::new();
    for (marketplace, record) in known {
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
        for entry in manifest.plugins {
            let id = format!("{}@{marketplace}", entry.name);
            if entry.name.is_empty() || entries.contains_key(&id) {
                continue;
            }
            entries.insert(id, classify(&entry, &dir, git_hosted));
        }
    }
    PluginCatalogue { entries }
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
                .and_then(|raw| serde_json::from_str::<PluginManifest>(&raw).ok())
                .and_then(|manifest| non_empty(manifest.version.as_deref()));
            manifest_version
                .or(entry_version)
                .map_or(CatalogueVersion::Untracked, CatalogueVersion::Version)
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
    !path.is_absolute() && !path.components().any(|c| c == Component::ParentDir)
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}
