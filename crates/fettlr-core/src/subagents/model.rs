use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::models::Scope;
use crate::skills::Provenance;

#[derive(Clone, Debug, Serialize)]
pub struct Subagent {
    pub id: String,
    /// The file's path relative to its agents root, extension included: `review/security.md` for a
    /// nested file, `reviewer.md` for a top-level one. Parking uses it as the destination leaf, so
    /// two subagents sharing a file name in different folders do not collide.
    pub relative_path: String,
    pub name: String,
    pub summary: Option<String>,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub effort: Option<String>,
    pub permission_mode: Option<String>,
    pub max_turns: Option<u32>,
    pub memory: Option<String>,
    pub scope: Scope,
    pub provenance: Provenance,
    pub path: PathBuf,
}

impl Subagent {
    pub fn from_fields(
        fields: &BTreeMap<String, String>,
        id: String,
        relative_path: String,
        stem: &str,
        scope: Scope,
        provenance: Provenance,
        path: PathBuf,
    ) -> Self {
        let text = |key: &str| {
            fields
                .get(key)
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        Self {
            id,
            relative_path,
            name: text("name").unwrap_or_else(|| stem.to_string()),
            summary: text("description"),
            model: text("model"),
            tools: text("tools")
                .map(|list| {
                    list.split(',')
                        .map(str::trim)
                        .filter(|tool| !tool.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            effort: text("effort"),
            permission_mode: text("permissionMode"),
            max_turns: text("maxTurns").and_then(|value| value.parse().ok()),
            memory: text("memory"),
            scope,
            provenance,
            path,
        }
    }
}
