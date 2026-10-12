use std::path::PathBuf;

use serde::Serialize;

use crate::models::Scope;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Provenance {
    Local,
    GitClone,
    Plugin { plugin_id: String },
}

#[derive(Clone, Debug, Serialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub scope: Scope,
    pub provenance: Provenance,
    pub path: PathBuf,
}
