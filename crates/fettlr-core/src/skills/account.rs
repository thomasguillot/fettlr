use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::discovery::{non_empty, skill_frontmatter_fields};
use crate::models::Scope;
use crate::paths::Paths;
use crate::skills::{Provenance, Skill};

pub(crate) fn account_skills(paths: &Paths, warnings: &mut Vec<String>) -> Vec<Skill> {
    if sync_is_off(&paths.settings_json()) {
        return Vec::new();
    }
    let Some(bucket) = newest_bucket(&paths.account_skills_dir()) else {
        return Vec::new();
    };
    let manifest_path = bucket.join("manifest.json");
    let Some(entries) = read_entries(&manifest_path) else {
        warnings.push(format!(
            "Could not read the Claude account skills listed in {}.",
            manifest_path.display()
        ));
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    for entry in &entries {
        let Some(name) = string_field(entry, "name") else {
            continue;
        };
        let Some(updated_at) = string_field(entry, "updatedAt") else {
            continue;
        };
        if !is_plain_folder_name(name) || is_anthropic(entry) || !seen.insert(name.to_string()) {
            continue;
        }
        let folder = bucket.join(name);
        if !folder.join("SKILL.md").is_file() {
            continue;
        }
        let fields = skill_frontmatter_fields(&folder, warnings);
        let manifest_description = string_field(entry, "description").map(str::to_string);
        skills.push(Skill {
            id: format!("account:{name}"),
            name: name.to_string(),
            title: non_empty(fields.get("name")),
            summary: non_empty(fields.get("description")).or(manifest_description),
            scope: Scope::User,
            provenance: Provenance::Account {
                updated_at: updated_at.to_string(),
            },
            path: folder,
        });
    }
    skills
}

fn sync_is_off(settings: &Path) -> bool {
    std::fs::read_to_string(settings)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("syncClaudeAiSkills").and_then(Value::as_bool))
        == Some(false)
}

fn newest_bucket(root: &Path) -> Option<PathBuf> {
    let mut buckets: Vec<(u64, String, PathBuf)> = std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            if name.starts_with('.') || !entry.path().is_dir() {
                return None;
            }
            let manifest = entry.path().join("manifest.json");
            if !manifest.is_file() {
                return None;
            }
            let updated = std::fs::read_to_string(&manifest)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .and_then(|value| value.get("lastUpdated").and_then(Value::as_u64))
                .unwrap_or(0);
            Some((updated, name, entry.path()))
        })
        .collect();
    buckets.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    buckets.into_iter().next().map(|(_, _, path)| path)
}

fn read_entries(manifest: &Path) -> Option<Vec<Value>> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let value = serde_json::from_str::<Value>(&text).ok()?;
    value.as_object()?.get("skills")?.as_array().cloned()
}

fn string_field<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn is_plain_folder_name(name: &str) -> bool {
    !name.contains(['/', '\\']) && !name.starts_with('.')
}

fn is_anthropic(entry: &Value) -> bool {
    string_field(entry, "source").is_some_and(|source| source.starts_with("anthropic"))
        || string_field(entry, "creatorType") == Some("anthropic")
}
