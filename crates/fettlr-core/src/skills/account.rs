use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::discovery::{non_empty, skill_frontmatter_fields};
use crate::models::Scope;
use crate::paths::Paths;
use crate::skills::{Provenance, Skill, is_real_dir};

pub(crate) fn account_skills(paths: &Paths, warnings: &mut Vec<String>) -> Vec<Skill> {
    if sync_is_off(&paths.settings_json()) {
        return Vec::new();
    }
    let Some(bucket) = newest_bucket(&paths.account_skills_dir()) else {
        return Vec::new();
    };
    let Some(entries) = bucket
        .manifest
        .as_ref()
        .and_then(|manifest| manifest.get("skills")?.as_array())
    else {
        warnings.push(format!(
            "Could not read the Claude account skills listed in {}.",
            bucket.manifest_path.display()
        ));
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    for entry in entries {
        let Some(name) = string_field(entry, "name") else {
            continue;
        };
        let Some(updated_at) = string_field(entry, "updatedAt") else {
            continue;
        };
        if !is_plain_folder_name(name) || is_anthropic(entry) || !seen.insert(name.to_string()) {
            continue;
        }
        let folder = bucket.path.join(name);
        if !is_real_dir(&folder) || !is_real_file(&folder.join("SKILL.md")) {
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

struct Bucket {
    name: String,
    path: PathBuf,
    manifest_path: PathBuf,
    manifest: Option<Value>,
}

impl Bucket {
    fn rank(&self) -> Option<f64> {
        let manifest = self.manifest.as_ref()?;
        Some(
            manifest
                .get("lastUpdated")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        )
    }
}

fn sync_is_off(settings: &Path) -> bool {
    std::fs::read_to_string(settings)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("syncClaudeAiSkills").and_then(Value::as_bool))
        == Some(false)
}

fn newest_bucket(root: &Path) -> Option<Bucket> {
    let mut buckets: Vec<Bucket> = std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            let path = entry.path();
            if name.starts_with('.') || !is_real_dir(&path) {
                return None;
            }
            let manifest_path = path.join("manifest.json");
            if !is_real_file(&manifest_path) {
                return None;
            }
            let manifest = std::fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .filter(Value::is_object);
            Some(Bucket {
                name,
                path,
                manifest_path,
                manifest,
            })
        })
        .collect();
    buckets.sort_by(|a, b| {
        b.rank()
            .partial_cmp(&a.rank())
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    buckets.into_iter().next()
}

fn is_real_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
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
