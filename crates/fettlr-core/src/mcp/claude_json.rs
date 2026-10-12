use std::path::Path;

use serde_json::{Map, Value};

use crate::cli_records::CoreError;
use crate::discovery::string_array as strings;
use crate::json_store::save_json_store;

const ENABLED_KEY: &str = "enabledMcpjsonServers";
const DISABLED_KEY: &str = "disabledMcpjsonServers";
/// Claude Code's disable list for the servers a project entry defines itself, kept apart from the
/// two `mcpjson` arrays above, which only ever describe the project's `.mcp.json` file.
const DISABLED_LOCAL_KEY: &str = "disabledMcpServers";

/// Reads the whole file, moves one name between the two arrays of one project, and writes the
/// object back: every other key Claude Code owns is carried across untouched.
pub fn set_project_mcpjson_server(
    claude_json: &Path,
    project_path: &str,
    server_name: &str,
    enabled: bool,
) -> Result<(), CoreError> {
    edit_project(claude_json, project_path, |project| {
        sort_mcpjson_name(project, claude_json, server_name, Some(enabled))
    })
}

/// Drops a name from the two `.mcp.json` arrays, for a server that is leaving the project file: a
/// name left behind silently disables the next server added under it, in Claude Code as well as
/// here. The local-scope list is a different namespace and is left alone.
pub fn forget_project_mcpjson_server(
    claude_json: &Path,
    project_path: &str,
    server_name: &str,
) -> Result<(), CoreError> {
    edit_project(claude_json, project_path, |project| {
        sort_mcpjson_name(project, claude_json, server_name, None)
    })
}

/// Drops a name from the local-scope disable list, for a server that is leaving the project's own
/// servers. The `.mcp.json` arrays are a different namespace and are left alone.
pub fn forget_project_local_server(
    claude_json: &Path,
    project_path: &str,
    server_name: &str,
) -> Result<(), CoreError> {
    edit_project(claude_json, project_path, |project| {
        set_listed(project, claude_json, DISABLED_LOCAL_KEY, server_name, false)
    })
}

/// Toggles a local-scope server, which Claude Code disables by name in a single list rather than
/// by moving it between an enabled and a disabled array.
pub fn set_project_mcp_server(
    claude_json: &Path,
    project_path: &str,
    server_name: &str,
    enabled: bool,
) -> Result<(), CoreError> {
    edit_project(claude_json, project_path, |project| {
        set_listed(
            project,
            claude_json,
            DISABLED_LOCAL_KEY,
            server_name,
            !enabled,
        )
    })
}

fn sort_mcpjson_name(
    project: &mut Map<String, Value>,
    claude_json: &Path,
    server_name: &str,
    enabled: Option<bool>,
) -> Result<(), CoreError> {
    set_listed(
        project,
        claude_json,
        ENABLED_KEY,
        server_name,
        enabled == Some(true),
    )?;
    set_listed(
        project,
        claude_json,
        DISABLED_KEY,
        server_name,
        enabled == Some(false),
    )
}

/// Adds or removes one name from one array, leaving the array absent if it was absent and is not
/// needed: an empty array Claude Code never wrote is noise in a file it owns.
///
/// A present-but-wrong-type value is refused rather than replaced, the same way `object_at` refuses
/// one: reading it as an empty list and writing that back discards whatever it actually held.
fn set_listed(
    project: &mut Map<String, Value>,
    claude_json: &Path,
    key: &str,
    server_name: &str,
    listed: bool,
) -> Result<(), CoreError> {
    let present = project.get(key);
    if present.is_some_and(|value| !value.is_array()) {
        return Err(malformed(claude_json));
    }
    let mut names = strings(present);
    names.retain(|name| name != server_name);
    if listed {
        names.push(server_name.to_string());
    }
    if !names.is_empty() || project.contains_key(key) {
        project.insert(key.to_string(), Value::from(names));
    }
    Ok(())
}

/// Claude Code rewrites `~/.claude.json` on every turn of any live session, so this is a
/// read-modify-write against a file with another writer. Re-reading when the file moved under us
/// does not make it atomic, but it turns the common case from a silently lost update into a
/// retried one, and a second collision into an error rather than a clobber.
fn edit_project(
    claude_json: &Path,
    project_path: &str,
    edit: impl Fn(&mut Map<String, Value>) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    for attempt in 0..2 {
        let before = fingerprint(claude_json);
        let text =
            std::fs::read_to_string(claude_json).map_err(|_| CoreError::UnreadableRootConfig {
                path: claude_json.display().to_string(),
            })?;
        let mut root = match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(root)) => root,
            _ => return Err(malformed(claude_json)),
        };

        let mut projects = object_at(root.get("projects"), claude_json)?;
        let original = projects.get(project_path).cloned();
        let mut project = object_at(original.as_ref(), claude_json)?;
        edit(&mut project)?;

        // Rewriting the file is never free: a write that changes nothing only widens the window
        // for someone else's update to be lost.
        let updated = Value::Object(project);
        if original.as_ref() == Some(&updated) {
            return Ok(());
        }

        if fingerprint(claude_json) != before {
            if attempt == 0 {
                continue;
            }
            return Err(CoreError::ContendedRootConfig {
                path: claude_json.display().to_string(),
            });
        }

        projects.insert(project_path.to_string(), updated);
        root.insert("projects".to_string(), Value::Object(projects));
        return save_json_store(claude_json, &Value::Object(root));
    }
    unreachable!("the loop returns on every path")
}

/// Enough to notice someone else's write between our read and our own. Not a lock.
fn fingerprint(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let data = std::fs::metadata(path).ok()?;
    Some((data.len(), data.modified().ok()?))
}

/// A present-but-wrong-type value is refused rather than defaulted: writing the file back would
/// discard whatever it held.
fn object_at(raw: Option<&Value>, claude_json: &Path) -> Result<Map<String, Value>, CoreError> {
    match raw {
        None => Ok(Map::new()),
        Some(Value::Object(object)) => Ok(object.clone()),
        Some(_) => Err(malformed(claude_json)),
    }
}

fn malformed(claude_json: &Path) -> CoreError {
    CoreError::MalformedRootConfig {
        path: claude_json.display().to_string(),
    }
}
