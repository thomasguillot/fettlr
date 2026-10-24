use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::civil_time::parse_rfc3339;
use crate::frontmatter::{markdown_body, parse_frontmatter};
use crate::models::Scope;
use crate::skills::{Provenance, Skill, skill_files};
use crate::updates::{is_older, numeric_fields};

const UNTRUSTED_EDIT_WINDOW_SECONDS: i64 = 300;

/// An account skill that a newer copy on this machine supersedes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AccountSkillBehind {
    pub account_id: String,
    pub local_id: String,
    pub local_version: Option<String>,
}

struct SkillCopy<'a> {
    skill: &'a Skill,
    files: BTreeMap<String, PathBuf>,
    version: Option<String>,
}

impl SkillCopy<'_> {
    fn fields(&self) -> Option<Vec<i64>> {
        self.version.as_deref().and_then(numeric_fields)
    }

    fn newest_mtime(&self) -> Option<i64> {
        self.files
            .values()
            .filter_map(|path| std::fs::metadata(path).ok()?.modified().ok())
            .filter_map(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_secs() as i64)
            .max()
    }
}

pub fn account_skills_behind(skills: &[Skill]) -> Vec<AccountSkillBehind> {
    let mut found: Vec<AccountSkillBehind> = skills
        .iter()
        .filter_map(|skill| match &skill.provenance {
            Provenance::Account { updated_at } => behind(skill, updated_at, skills),
            _ => None,
        })
        .collect();
    found.sort_by(|a, b| a.account_id.cmp(&b.account_id));
    found
}

fn behind(account: &Skill, updated_at: &str, skills: &[Skill]) -> Option<AccountSkillBehind> {
    let account_copy = read_copy(account)?;
    let candidates: Vec<SkillCopy> = skills
        .iter()
        .filter(|candidate| is_candidate(candidate, account))
        .filter_map(read_copy)
        .collect();
    if candidates
        .iter()
        .any(|candidate| same_content(&account_copy, candidate))
    {
        return None;
    }
    let uploaded = parse_rfc3339(updated_at);
    candidates
        .iter()
        .filter(|candidate| is_newer(&account_copy, candidate, uploaded))
        .max_by(|a, b| preference(a, b))
        .map(|best| AccountSkillBehind {
            account_id: account.id.clone(),
            local_id: best.skill.id.clone(),
            local_version: best.version.clone(),
        })
}

fn is_candidate(candidate: &Skill, account: &Skill) -> bool {
    if matches!(candidate.provenance, Provenance::Account { .. }) {
        return false;
    }
    let effective_name = candidate
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .unwrap_or(candidate.name.as_str());
    let same_name = effective_name == account.name;
    let reachable =
        matches!(candidate.provenance, Provenance::Plugin { .. }) || candidate.scope == Scope::User;
    same_name && reachable
}

fn read_copy(skill: &Skill) -> Option<SkillCopy<'_>> {
    let files = skill_files(&skill.path).ok()?.files;
    let markdown = std::fs::read_to_string(files.get("SKILL.md")?).ok()?;
    let version = parse_frontmatter(&markdown)
        .remove("version")
        .filter(|version| !version.is_empty());
    Some(SkillCopy {
        skill,
        files,
        version,
    })
}

fn same_content(a: &SkillCopy, b: &SkillCopy) -> bool {
    if !a.files.keys().eq(b.files.keys()) {
        return false;
    }
    a.files.iter().all(|(key, path)| {
        let other = &b.files[key];
        if key == "SKILL.md" {
            match (
                std::fs::read_to_string(path),
                std::fs::read_to_string(other),
            ) {
                (Ok(left), Ok(right)) => {
                    markdown_body(&left) == markdown_body(&right)
                        && frontmatter_without_version(&left) == frontmatter_without_version(&right)
                }
                _ => false,
            }
        } else {
            match (std::fs::read(path), std::fs::read(other)) {
                (Ok(left), Ok(right)) => left == right,
                _ => false,
            }
        }
    })
}

fn frontmatter_without_version(markdown: &str) -> BTreeMap<String, String> {
    let mut fields = parse_frontmatter(markdown);
    fields.remove("version");
    fields
}

fn is_newer(account: &SkillCopy, local: &SkillCopy, uploaded: Option<i64>) -> bool {
    match (&account.version, &local.version) {
        (Some(_), Some(_)) => match (account.fields(), local.fields()) {
            (Some(theirs), Some(ours)) => is_older(&theirs, &ours),
            _ => false,
        },
        (None, Some(_)) => local.fields().is_some(),
        (Some(_), None) => false,
        (None, None) => match (uploaded, local.newest_mtime()) {
            (Some(uploaded), Some(edited)) => edited - uploaded > UNTRUSTED_EDIT_WINDOW_SECONDS,
            _ => false,
        },
    }
}

fn preference(a: &SkillCopy, b: &SkillCopy) -> Ordering {
    let by_version = match (a.fields(), b.fields()) {
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (Some(left), Some(right)) => {
            if is_older(&left, &right) {
                Ordering::Less
            } else if is_older(&right, &left) {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
        (None, None) => Ordering::Equal,
    };
    by_version
        .then_with(|| a.newest_mtime().cmp(&b.newest_mtime()))
        .then_with(|| b.skill.id.cmp(&a.skill.id))
}
