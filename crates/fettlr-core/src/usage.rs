use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::civil_time;
use crate::skills::{Provenance, Skill};

/// How often Claude Code has reached for one skill or plugin, as it records it itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub count: u64,
    pub last_used_at: Option<String>,
}

/// The `skillUsage` and `pluginUsage` maps out of `~/.claude.json`.
///
/// An absent entry and a recorded zero mean different things and are kept apart all the way to the
/// window: zero is a skill that has never fired, which is a finding, while absence is a skill
/// Claude Code holds no record of, which is not.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageIndex {
    skills: BTreeMap<String, Usage>,
    plugins: BTreeMap<String, Usage>,
}

impl UsageIndex {
    pub fn from_root(root: &Map<String, Value>) -> Self {
        Self {
            skills: entries_from(root.get("skillUsage")),
            plugins: entries_from(root.get("pluginUsage")),
        }
    }

    pub fn for_skill(&self, skill: &Skill) -> Option<&Usage> {
        self.skills.get(&skill_key(skill))
    }

    pub fn for_plugin(&self, plugin_id: &str) -> Option<&Usage> {
        self.plugins.get(plugin_id)
    }
}

/// Claude Code keys a plugin's skill by the bare plugin name, while `claude plugin list --json`
/// reports the id as `name@marketplace`, so the marketplace is dropped here. A loose skill is
/// keyed by its folder name alone. The two forms are never merged: `code-review` and
/// `mattpocock-skills:code-review` are different skills, and summing them would credit each with
/// the other's history.
fn skill_key(skill: &Skill) -> String {
    match &skill.provenance {
        Provenance::Plugin { plugin_id } => {
            // Split at the last `@`, the way `split_plugin_id` derives a plugin's name, so an id
            // that carries one in its name resolves to the same bare name in both places.
            let bare = match plugin_id.rfind('@') {
                Some(index) => &plugin_id[..index],
                None => plugin_id.as_str(),
            };
            format!("{bare}:{}", skill.name)
        }
        _ => skill.name.clone(),
    }
}

fn entries_from(raw: Option<&Value>) -> BTreeMap<String, Usage> {
    let Some(entries) = raw.and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    entries
        .iter()
        .filter_map(|(name, value)| usage_from(value).map(|usage| (name.clone(), usage)))
        .collect()
}

fn usage_from(value: &Value) -> Option<Usage> {
    let entry = value.as_object()?;
    let count = entry.get("usageCount")?.as_u64()?;
    Some(Usage {
        count,
        last_used_at: entry
            .get("lastUsedAt")
            .and_then(Value::as_i64)
            .map(|millis| civil_time::format_rfc3339(millis.div_euclid(1_000))),
    })
}
