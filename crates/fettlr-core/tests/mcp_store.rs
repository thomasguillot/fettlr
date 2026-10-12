use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fettlr_core::testing::{FakeCliRunner, FakeClock, FakeMonotonicClock, TempTree};
use fettlr_core::{
    ClaudeCli, Clock, CoreError, DiscoveredConfig, McpMutating, McpOrigin, McpServer,
    McpServerConfig, McpStore, Paths, PluginRecord, ProjectEntry, Scope,
};
use serde_json::{Map, Value, json};

const PROJECT: &str = "/fake/home/Sites/example-project";
const OTHER_PROJECT: &str = "/fake/home/Sites/other-project";
const DISABLED_AT: &str = "2026-08-23T09:00:00Z";

#[derive(Clone, Debug, PartialEq)]
enum Call {
    Add {
        name: String,
        config_json: String,
        scope: Scope,
        local: bool,
    },
    Remove {
        name: String,
        scope: Scope,
        local: bool,
    },
}

struct FakeState {
    calls: Vec<Call>,
    add_failures: usize,
    adds: usize,
    remove_fails: bool,
}

struct FakeMcpMutating {
    state: Mutex<FakeState>,
}

impl FakeMcpMutating {
    fn new() -> Self {
        Self::with(0, false)
    }

    fn failing_adds(add_failures: usize) -> Self {
        Self::with(add_failures, false)
    }

    fn failing_removes() -> Self {
        Self::with(0, true)
    }

    fn with(add_failures: usize, remove_fails: bool) -> Self {
        Self {
            state: Mutex::new(FakeState {
                calls: Vec::new(),
                add_failures,
                adds: 0,
                remove_fails,
            }),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.state.lock().expect("lock poisoned").calls.clone()
    }
}

fn cli_failure() -> CoreError {
    CoreError::Cli {
        command: "claude mcp".to_string(),
        stderr: "boom".to_string(),
    }
}

fn is_cli_failure(error: &CoreError) -> bool {
    matches!(error, CoreError::Cli { stderr, .. } if stderr == "boom")
}

impl McpMutating for FakeMcpMutating {
    fn mcp_add_json(
        &self,
        name: &str,
        config_json: &str,
        scope: &Scope,
        local: bool,
    ) -> Result<(), CoreError> {
        let mut state = self.state.lock().expect("lock poisoned");
        state.calls.push(Call::Add {
            name: name.to_string(),
            config_json: config_json.to_string(),
            scope: scope.clone(),
            local,
        });
        state.adds += 1;
        if state.adds <= state.add_failures {
            return Err(cli_failure());
        }
        Ok(())
    }

    fn mcp_remove(&self, name: &str, scope: &Scope, local: bool) -> Result<(), CoreError> {
        let mut state = self.state.lock().expect("lock poisoned");
        state.calls.push(Call::Remove {
            name: name.to_string(),
            scope: scope.clone(),
            local,
        });
        if state.remove_fails {
            return Err(cli_failure());
        }
        Ok(())
    }
}

type Hook = Box<dyn Fn() + Send + Sync>;

/// Runs a caller's work while a removal is in flight, standing in for the refresh Claude Code's own
/// write to `~/.claude.json` sets off before the removal has been applied.
struct InterruptingMutator {
    hook: Mutex<Option<Hook>>,
}

impl InterruptingMutator {
    fn new() -> Self {
        Self {
            hook: Mutex::new(None),
        }
    }

    fn set_hook(&self, hook: Hook) {
        *self.hook.lock().expect("lock poisoned") = Some(hook);
    }

    fn fire(&self) {
        let hook = self.hook.lock().expect("lock poisoned");
        if let Some(hook) = hook.as_ref() {
            hook();
        }
    }
}

impl McpMutating for InterruptingMutator {
    fn mcp_add_json(&self, _: &str, _: &str, _: &Scope, _: bool) -> Result<(), CoreError> {
        self.fire();
        Ok(())
    }

    fn mcp_remove(&self, _: &str, _: &Scope, _: bool) -> Result<(), CoreError> {
        self.fire();
        Ok(())
    }
}

/// Holds the first caller inside the store's parking window long enough for a second one to reach
/// it, so an unsynchronised read-modify-write of `mcp-disabled.json` loses a record.
struct SlowFirstClock {
    stamp: String,
    delay: Duration,
    used: Mutex<bool>,
}

impl SlowFirstClock {
    fn new(stamp: &str, delay: Duration) -> Self {
        Self {
            stamp: stamp.to_string(),
            delay,
            used: Mutex::new(false),
        }
    }
}

impl Clock for SlowFirstClock {
    fn now_rfc3339(&self) -> String {
        let first = {
            let mut used = self.used.lock().expect("lock poisoned");
            let first = !*used;
            *used = true;
            first
        };
        if first {
            std::thread::sleep(self.delay);
        }
        self.stamp.clone()
    }
}

fn config(value: Value) -> McpServerConfig {
    serde_json::from_value(value).expect("decode server config")
}

fn context7() -> McpServerConfig {
    config(json!({
        "command": "npx",
        "args": ["-y", "@upstash/context7-mcp"],
        "env": {"CONTEXT7_API_KEY": "abc123"},
    }))
}

fn playwright() -> McpServerConfig {
    config(json!({"command": "npx", "args": ["@playwright/mcp"]}))
}

fn figma() -> McpServerConfig {
    config(json!({
        "type": "http",
        "url": "https://mcp.figma.com/mcp",
        "headers": {"X-Key": "secret"},
    }))
}

fn legacy() -> McpServerConfig {
    config(json!({"command": "old-server"}))
}

fn fresh() -> McpServerConfig {
    config(json!({"type": "sse", "url": "https://example.com/sse"}))
}

fn plugin(id: &str, enabled: bool, servers: Value, scope: &str) -> PluginRecord {
    serde_json::from_value(json!({
        "id": id,
        "version": "1.0.0",
        "scope": scope,
        "projectPath": PROJECT,
        "enabled": enabled,
        "installPath": format!("/fake/home/.claude/plugins/cache/{id}"),
        "mcpServers": servers,
    }))
    .expect("decode plugin record")
}

struct Sample {
    user_servers: BTreeMap<String, McpServerConfig>,
    enabled_mcpjson: Vec<String>,
    disabled_mcpjson: Vec<String>,
    disabled_local: Vec<String>,
    read_at: Instant,
}

impl Sample {
    fn new(read_at: Instant) -> Self {
        Self {
            user_servers: BTreeMap::from([("context7".to_string(), context7())]),
            enabled_mcpjson: vec!["figma".to_string()],
            disabled_mcpjson: vec!["legacy".to_string()],
            disabled_local: Vec::new(),
            read_at,
        }
    }

    fn without_user_servers(mut self) -> Self {
        self.user_servers = BTreeMap::new();
        self
    }

    fn mcpjson(mut self, enabled: &[&str], disabled: &[&str]) -> Self {
        self.enabled_mcpjson = enabled.iter().map(|name| name.to_string()).collect();
        self.disabled_mcpjson = disabled.iter().map(|name| name.to_string()).collect();
        self
    }

    fn local_disabled(mut self, disabled: &[&str]) -> Self {
        self.disabled_local = disabled.iter().map(|name| name.to_string()).collect();
        self
    }

    fn build(self) -> DiscoveredConfig {
        DiscoveredConfig {
            user_mcp_servers: self.user_servers,
            projects: vec![ProjectEntry {
                path: PROJECT.to_string(),
                exists: true,
                has_trust_dialog_accepted: true,
                local_mcp_servers: BTreeMap::from([("playwright".to_string(), playwright())]),
                enabled_mcpjson_servers: self.enabled_mcpjson,
                disabled_mcpjson_servers: self.disabled_mcpjson,
                disabled_mcp_servers: self.disabled_local,
            }],
            project_mcp_json: BTreeMap::from([(
                PROJECT.to_string(),
                BTreeMap::from([
                    ("figma".to_string(), figma()),
                    ("legacy".to_string(), legacy()),
                    ("fresh".to_string(), fresh()),
                ]),
            )]),
            read_at: self.read_at,
            ..Default::default()
        }
    }
}

struct Harness {
    tree: TempTree,
    cli: Arc<FakeMcpMutating>,
    store: McpStore,
    base: Instant,
}

impl Harness {
    fn new() -> Self {
        Self::with_cli(Arc::new(FakeMcpMutating::new()))
    }

    fn with_cli(cli: Arc<FakeMcpMutating>) -> Self {
        let tree = TempTree::new();
        let base = Instant::now();
        let store = McpStore::with_clocks(
            &tree.paths,
            cli.clone(),
            Arc::new(FakeClock::new(DISABLED_AT)),
            Arc::new(FakeMonotonicClock::new(base + Duration::from_secs(1))),
        );
        Self {
            tree,
            cli,
            store,
            base,
        }
    }

    /// A snapshot read after Fettlr's own last change: the store may trust it.
    fn sample(&self) -> Sample {
        Sample::new(self.base + Duration::from_secs(2))
    }

    /// A snapshot read before Fettlr's own last change.
    fn stale_sample(&self) -> Sample {
        Sample::new(self.base)
    }

    fn assemble(&self, config: &DiscoveredConfig) -> Vec<McpServer> {
        self.store.assemble(config, &[]).expect("assembles")
    }

    fn calls(&self) -> Vec<Call> {
        self.cli.calls()
    }

    fn disabled_json(&self) -> PathBuf {
        self.tree.paths.mcp_disabled_file()
    }

    fn records(&self) -> Vec<Value> {
        let raw = std::fs::read_to_string(self.disabled_json()).expect("read disabled store");
        serde_json::from_str(&raw).expect("decode disabled store")
    }
}

fn server(servers: &[McpServer], id: &str) -> McpServer {
    servers
        .iter()
        .find(|server| server.id == id)
        .unwrap_or_else(|| panic!("no server with id {id}"))
        .clone()
}

fn ids(servers: &[McpServer]) -> Vec<String> {
    servers.iter().map(|server| server.id.clone()).collect()
}

fn added(call: &Call) -> (String, McpServerConfig, Scope, bool) {
    match call {
        Call::Add {
            name,
            config_json,
            scope,
            local,
        } => (
            name.clone(),
            serde_json::from_str(config_json).expect("decode the config json"),
            scope.clone(),
            *local,
        ),
        other => panic!("expected an add-json call, got {other:?}"),
    }
}

fn project_entry(tree: &TempTree) -> Value {
    let raw = std::fs::read_to_string(&tree.paths.claude_json).expect("read claude.json");
    let root: Value = serde_json::from_str(&raw).expect("decode claude.json");
    root.get("projects")
        .and_then(|projects| projects.get(PROJECT))
        .cloned()
        .expect("project entry")
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn write_project_arrays(tree: &TempTree) {
    let mut projects = Map::new();
    projects.insert(
        PROJECT.to_string(),
        json!({
            "lastCost": 1.25,
            "enabledMcpjsonServers": ["figma"],
            "disabledMcpjsonServers": ["legacy"],
        }),
    );
    tree.write_claude_json(json!({"numStartups": 42, "projects": projects}));
    std::fs::set_permissions(
        &tree.paths.claude_json,
        std::fs::Permissions::from_mode(0o600),
    )
    .expect("chmod 0600");
}

#[test]
fn assemble_merges_all_four_origins() {
    let harness = Harness::new();
    let plugins = [plugin(
        "accesslint@accesslint",
        true,
        json!({"accesslint": {"command": "npx", "args": ["-y", "@accesslint/mcp"]}}),
        "user",
    )];

    let servers = harness
        .store
        .assemble(&harness.sample().build(), &plugins)
        .expect("assembles");

    assert_eq!(
        ids(&servers),
        [
            format!("local:{PROJECT}/playwright"),
            "plugin:accesslint@accesslint/accesslint".to_string(),
            format!("project:{PROJECT}/figma"),
            format!("project:{PROJECT}/fresh"),
            format!("project:{PROJECT}/legacy"),
            "user/context7".to_string(),
        ]
    );

    let user = server(&servers, "user/context7");
    assert_eq!(user.origin, McpOrigin::UserScope);
    assert_eq!(user.name, "context7");
    assert_eq!(user.config, context7());
    assert_eq!(user.transport, "stdio");
    assert!(user.is_enabled);

    let local = server(&servers, &format!("local:{PROJECT}/playwright"));
    assert_eq!(
        local.origin,
        McpOrigin::LocalScope {
            project: PROJECT.to_string()
        }
    );
    assert_eq!(local.config, playwright());
    assert_eq!(local.transport, "stdio");
    assert!(local.is_enabled);

    let project_file = server(&servers, &format!("project:{PROJECT}/figma"));
    assert_eq!(
        project_file.origin,
        McpOrigin::ProjectFile {
            project: PROJECT.to_string()
        }
    );
    assert_eq!(project_file.transport, "http");
    assert_eq!(
        server(&servers, &format!("project:{PROJECT}/fresh")).transport,
        "sse"
    );
    assert_eq!(
        server(&servers, "plugin:accesslint@accesslint/accesslint").origin,
        McpOrigin::Plugin {
            plugin_id: "accesslint@accesslint".to_string()
        }
    );
}

#[test]
fn mcp_json_servers_follow_the_project_arrays() {
    let harness = Harness::new();

    let servers = harness.assemble(&harness.sample().build());

    assert!(server(&servers, &format!("project:{PROJECT}/figma")).is_enabled);
    assert!(!server(&servers, &format!("project:{PROJECT}/legacy")).is_enabled);
    assert!(server(&servers, &format!("project:{PROJECT}/fresh")).is_enabled);
}

#[test]
fn an_mcp_json_server_in_both_arrays_counts_as_disabled() {
    let harness = Harness::new();

    let servers = harness.assemble(&harness.sample().mcpjson(&["figma"], &["figma"]).build());

    assert!(!server(&servers, &format!("project:{PROJECT}/figma")).is_enabled);
}

#[test]
fn plugin_servers_follow_their_plugins_enabled_flag() {
    let harness = Harness::new();
    let plugins = [
        plugin(
            "linear@official",
            false,
            json!({"linear": {"type": "http", "url": "https://mcp.linear.app/mcp"}}),
            "user",
        ),
        plugin(
            "context7@official",
            false,
            json!({"context7-plugin": {"command": "npx"}}),
            "project",
        ),
        plugin(
            "context7@official",
            true,
            json!({"context7-plugin": {"command": "npx"}}),
            "user",
        ),
    ];

    let servers = harness
        .store
        .assemble(&DiscoveredConfig::default(), &plugins)
        .expect("assembles");

    assert!(!server(&servers, "plugin:linear@official/linear").is_enabled);
    assert_eq!(
        servers
            .iter()
            .filter(|server| server.id == "plugin:context7@official/context7-plugin")
            .count(),
        1
    );
    assert!(server(&servers, "plugin:context7@official/context7-plugin").is_enabled);
}

#[test]
fn parked_user_servers_appear_disabled() {
    let harness = Harness::new();
    let parked = harness.assemble(&harness.sample().build());
    harness
        .store
        .set_enabled(&server(&parked, "user/context7"), false)
        .expect("parks the server");

    let servers = harness.assemble(&harness.sample().without_user_servers().build());

    let disabled = server(&servers, "user/context7");
    assert!(!disabled.is_enabled);
    assert_eq!(disabled.config, context7());
    assert_eq!(disabled.origin, McpOrigin::UserScope);
}

#[test]
fn a_parked_record_is_ignored_once_the_server_is_live_again() {
    let harness = Harness::new();
    let parked = harness.assemble(&harness.sample().build());
    harness
        .store
        .set_enabled(&server(&parked, "user/context7"), false)
        .expect("parks the server");

    let servers = harness.assemble(&harness.sample().build());

    assert_eq!(
        servers
            .iter()
            .filter(|server| server.id == "user/context7")
            .count(),
        1
    );
    assert!(server(&servers, "user/context7").is_enabled);
}

#[test]
fn assemble_prunes_a_record_whose_server_is_live_again() {
    let harness = Harness::new();
    let parked = harness.assemble(&harness.sample().build());
    harness
        .store
        .set_enabled(&server(&parked, "user/context7"), false)
        .expect("parks the server");
    assert_eq!(harness.records().len(), 1);

    harness.assemble(&harness.sample().build());

    assert!(harness.records().is_empty());
}

#[test]
fn assemble_of_a_config_read_before_a_disable_keeps_the_record() {
    let harness = Harness::new();
    let live = harness.assemble(&harness.sample().build());
    let snapshot = harness.stale_sample().build();

    harness
        .store
        .set_enabled(&server(&live, "user/context7"), false)
        .expect("parks the server");
    harness.assemble(&snapshot);

    assert_eq!(harness.records().len(), 1);
}

#[test]
fn assemble_during_a_disable_keeps_the_record_it_has_not_seen_applied() {
    let tree = TempTree::new();
    let base = Instant::now();
    let cli = Arc::new(InterruptingMutator::new());
    let store = Arc::new(McpStore::with_clocks(
        &tree.paths,
        cli.clone(),
        Arc::new(FakeClock::new(DISABLED_AT)),
        Arc::new(FakeMonotonicClock::new(base + Duration::from_secs(1))),
    ));
    let live = store
        .assemble(&Sample::new(base + Duration::from_secs(2)).build(), &[])
        .expect("assembles");

    let weak = Arc::downgrade(&store);
    let still_listed = Sample::new(base + Duration::from_secs(2)).build();
    cli.set_hook(Box::new(move || {
        if let Some(store) = weak.upgrade() {
            let _ = store.assemble(&still_listed, &[]);
        }
    }));

    store
        .set_enabled(&server(&live, "user/context7"), false)
        .expect("parks the server");

    let raw = std::fs::read_to_string(tree.paths.mcp_disabled_file()).expect("read disabled store");
    let records: Vec<Value> = serde_json::from_str(&raw).expect("decode disabled store");
    assert_eq!(records.len(), 1);

    let after = store
        .assemble(
            &Sample::new(base + Duration::from_secs(3))
                .without_user_servers()
                .build(),
            &[],
        )
        .expect("assembles");
    assert!(!server(&after, "user/context7").is_enabled);
}

#[test]
fn a_stale_record_does_not_swallow_a_remove() {
    let harness = Harness::new();
    let live = harness.assemble(&harness.sample().build());
    harness
        .store
        .set_enabled(&server(&live, "user/context7"), false)
        .expect("parks the server");

    harness
        .store
        .remove(&McpServer::new(
            "context7",
            McpOrigin::UserScope,
            context7(),
            true,
        ))
        .expect("removes the server");

    assert_eq!(
        harness.calls(),
        [
            Call::Remove {
                name: "context7".to_string(),
                scope: Scope::User,
                local: false
            },
            Call::Remove {
                name: "context7".to_string(),
                scope: Scope::User,
                local: false
            },
        ]
    );
}

#[test]
fn a_stale_record_does_not_swallow_an_edit() {
    let harness = Harness::new();
    let live = harness.assemble(&harness.sample().build());
    harness
        .store
        .set_enabled(&server(&live, "user/context7"), false)
        .expect("parks the server");

    harness
        .store
        .edit(
            &McpServer::new("context7", McpOrigin::UserScope, context7(), true),
            "context7",
            &playwright(),
        )
        .expect("edits the server");

    let calls = harness.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(
        calls[1],
        Call::Remove {
            name: "context7".to_string(),
            scope: Scope::User,
            local: false
        }
    );
    let (name, config, scope, local) = added(&calls[2]);
    assert_eq!(name, "context7");
    assert_eq!(scope, Scope::User);
    assert!(!local);
    assert_eq!(config, playwright());
}

#[test]
fn disabling_a_user_server_parks_the_config_and_removes_it() {
    let harness = Harness::new();
    let servers = harness.assemble(&harness.sample().build());

    harness
        .store
        .set_enabled(&server(&servers, "user/context7"), false)
        .expect("parks the server");

    assert_eq!(
        harness.calls(),
        [Call::Remove {
            name: "context7".to_string(),
            scope: Scope::User,
            local: false
        }]
    );
    let records = harness.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], json!("user/context7"));
    assert_eq!(records[0]["name"], json!("context7"));
    assert_eq!(records[0]["origin"], json!("UserScope"));
    assert_eq!(
        records[0]["config"],
        serde_json::to_value(context7()).expect("serialize config")
    );
    assert_eq!(records[0]["disabled_at"], json!(DISABLED_AT));
}

#[test]
fn enabling_a_user_server_re_adds_the_stored_config() {
    let harness = Harness::new();
    let live = harness.assemble(&harness.sample().build());
    harness
        .store
        .set_enabled(&server(&live, "user/context7"), false)
        .expect("parks the server");
    let parked = harness.assemble(&harness.sample().without_user_servers().build());

    harness
        .store
        .set_enabled(&server(&parked, "user/context7"), true)
        .expect("re-adds the server");

    let calls = harness.calls();
    assert_eq!(calls.len(), 2);
    let (name, config, scope, local) = added(&calls[1]);
    assert_eq!(name, "context7");
    assert_eq!(scope, Scope::User);
    assert!(!local);
    assert_eq!(config, context7());
    let Call::Add { config_json, .. } = &calls[1] else {
        panic!("expected an add-json call");
    };
    assert_eq!(
        serde_json::from_str::<Value>(config_json).expect("decode"),
        serde_json::to_value(context7()).expect("serialize config")
    );
    assert!(harness.records().is_empty());
}

#[test]
fn disabling_a_local_server_lists_its_name_without_the_cli() {
    let harness = Harness::new();
    write_project_arrays(&harness.tree);
    let servers = harness.assemble(&harness.sample().build());

    harness
        .store
        .set_enabled(
            &server(&servers, &format!("local:{PROJECT}/playwright")),
            false,
        )
        .expect("lists the server as disabled");

    assert!(harness.calls().is_empty());
    assert!(!harness.disabled_json().exists());
    let entry = project_entry(&harness.tree);
    assert_eq!(strings(entry.get("disabledMcpServers")), ["playwright"]);
    assert_eq!(entry.get("lastCost"), Some(&json!(1.25)));
}

#[test]
fn enabling_a_local_server_drops_its_name_without_the_cli() {
    let harness = Harness::new();
    write_project_arrays(&harness.tree);
    let disabled = harness.assemble(&harness.sample().local_disabled(&["playwright"]).build());

    harness
        .store
        .set_enabled(
            &server(&disabled, &format!("local:{PROJECT}/playwright")),
            true,
        )
        .expect("drops the server from the disabled list");

    assert!(harness.calls().is_empty());
    let entry = project_entry(&harness.tree);
    assert!(strings(entry.get("disabledMcpServers")).is_empty());
}

#[test]
fn a_local_server_claude_code_disabled_is_shown_disabled() {
    let harness = Harness::new();

    let servers = harness.assemble(&harness.sample().local_disabled(&["playwright"]).build());

    let listed = server(&servers, &format!("local:{PROJECT}/playwright"));
    assert!(!listed.is_enabled);
}

#[test]
fn disabling_a_project_file_server_rewrites_the_arrays_without_the_cli() {
    let harness = Harness::new();
    write_project_arrays(&harness.tree);
    let servers = harness.assemble(&harness.sample().build());

    harness
        .store
        .set_enabled(
            &server(&servers, &format!("project:{PROJECT}/figma")),
            false,
        )
        .expect("rewrites the arrays");

    assert!(harness.calls().is_empty());
    assert!(!harness.disabled_json().exists());
    let entry = project_entry(&harness.tree);
    assert_eq!(
        strings(entry.get("disabledMcpjsonServers")),
        ["legacy", "figma"]
    );
    assert!(strings(entry.get("enabledMcpjsonServers")).is_empty());
    assert_eq!(entry.get("lastCost"), Some(&json!(1.25)));
    assert_eq!(
        std::fs::metadata(&harness.tree.paths.claude_json)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn enabling_a_project_file_server_rewrites_the_arrays() {
    let harness = Harness::new();
    write_project_arrays(&harness.tree);
    let servers = harness.assemble(&harness.sample().build());

    harness
        .store
        .set_enabled(
            &server(&servers, &format!("project:{PROJECT}/legacy")),
            true,
        )
        .expect("rewrites the arrays");

    assert!(harness.calls().is_empty());
    let entry = project_entry(&harness.tree);
    assert!(strings(entry.get("disabledMcpjsonServers")).is_empty());
    assert_eq!(
        strings(entry.get("enabledMcpjsonServers")),
        ["figma", "legacy"]
    );
}

#[test]
fn set_enabled_does_nothing_when_the_state_already_matches() {
    let harness = Harness::new();
    let servers = harness.assemble(&harness.sample().build());

    harness
        .store
        .set_enabled(&server(&servers, "user/context7"), true)
        .expect("does nothing");

    assert!(harness.calls().is_empty());
    assert!(!harness.disabled_json().exists());
}

#[test]
fn a_failed_remove_leaves_no_disabled_record() {
    let harness = Harness::with_cli(Arc::new(FakeMcpMutating::failing_removes()));
    let servers = harness.assemble(&harness.sample().build());

    let error = harness
        .store
        .set_enabled(&server(&servers, "user/context7"), false)
        .expect_err("the remove fails");

    assert!(is_cli_failure(&error), "unexpected error {error:?}");
    assert!(harness.records().is_empty());
}

#[test]
fn plugin_servers_refuse_every_mutation() {
    let harness = Harness::new();
    let plugins = [plugin(
        "accesslint@accesslint",
        true,
        json!({"accesslint": {"command": "npx"}}),
        "user",
    )];
    let servers = harness
        .store
        .assemble(&DiscoveredConfig::default(), &plugins)
        .expect("assembles");
    let managed = server(&servers, "plugin:accesslint@accesslint/accesslint");

    let toggle = harness
        .store
        .set_enabled(&managed, false)
        .expect_err("refuses the toggle");
    let edit = harness
        .store
        .edit(&managed, "renamed", &playwright())
        .expect_err("refuses the edit");
    let remove = harness
        .store
        .remove(&managed)
        .expect_err("refuses the remove");
    let add = harness
        .store
        .add("renamed", &playwright(), &managed.origin)
        .expect_err("refuses the add");

    for error in [toggle, edit, remove, add] {
        match error {
            CoreError::ManagedByPlugin { plugin_id } => {
                assert_eq!(plugin_id, "accesslint@accesslint");
            }
            other => panic!("expected a managed-by-plugin error, got {other:?}"),
        }
    }
    assert!(harness.calls().is_empty());
}

#[test]
fn add_rejects_a_duplicate_name_in_the_same_origin_before_calling_the_cli() {
    let harness = Harness::new();
    harness.assemble(&harness.sample().build());

    let error = harness
        .store
        .add("context7", &playwright(), &McpOrigin::UserScope)
        .expect_err("refuses the duplicate");

    match error {
        CoreError::DuplicateMcpName { name } => assert_eq!(name, "context7"),
        other => panic!("expected a duplicate-name error, got {other:?}"),
    }
    assert!(harness.calls().is_empty());
}

#[test]
fn add_rejects_a_name_that_is_only_parked() {
    let harness = Harness::new();
    let live = harness.assemble(&harness.sample().build());
    harness
        .store
        .set_enabled(&server(&live, "user/context7"), false)
        .expect("parks the server");
    harness.assemble(&harness.sample().without_user_servers().build());

    let error = harness
        .store
        .add("context7", &playwright(), &McpOrigin::UserScope)
        .expect_err("refuses the duplicate");

    assert!(matches!(error, CoreError::DuplicateMcpName { .. }));
    assert_eq!(harness.calls().len(), 1);
}

#[test]
fn add_sends_the_encoded_config_at_the_requested_scope() {
    let harness = Harness::new();
    harness.assemble(&harness.sample().build());

    harness
        .store
        .add(
            "context7",
            &figma(),
            &McpOrigin::ProjectFile {
                project: OTHER_PROJECT.to_string(),
            },
        )
        .expect("adds the server");

    let calls = harness.calls();
    let (name, config, scope, local) = added(calls.first().expect("a call"));
    assert_eq!(name, "context7");
    assert_eq!(
        scope,
        Scope::Project {
            path: OTHER_PROJECT.to_string()
        }
    );
    assert!(!local);
    assert_eq!(config, figma());
    let Call::Add { config_json, .. } = &calls[0] else {
        panic!("expected an add-json call");
    };
    assert!(!config_json.contains("\\/"));
}

#[test]
fn edit_removes_then_adds_under_the_new_name() {
    let harness = Harness::new();
    let servers = harness.assemble(&harness.sample().build());
    let renamed = config(json!({"type": "http", "url": "https://mcp.figma.com/v2"}));

    harness
        .store
        .edit(
            &server(&servers, &format!("project:{PROJECT}/figma")),
            "figma-v2",
            &renamed,
        )
        .expect("edits the server");

    let calls = harness.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0],
        Call::Remove {
            name: "figma".to_string(),
            scope: Scope::Project {
                path: PROJECT.to_string()
            },
            local: false
        }
    );
    let (name, config, scope, local) = added(&calls[1]);
    assert_eq!(name, "figma-v2");
    assert_eq!(
        scope,
        Scope::Project {
            path: PROJECT.to_string()
        }
    );
    assert!(!local);
    assert_eq!(config, renamed);
}

#[test]
fn edit_keeps_a_disabled_project_file_server_disabled() {
    let harness = Harness::new();
    write_project_arrays(&harness.tree);
    let servers = harness.assemble(&harness.sample().build());

    harness
        .store
        .edit(
            &server(&servers, &format!("project:{PROJECT}/legacy")),
            "legacy",
            &config(json!({"command": "newer-server"})),
        )
        .expect("edits the server");

    let entry = project_entry(&harness.tree);
    assert_eq!(strings(entry.get("disabledMcpjsonServers")), ["legacy"]);
    assert_eq!(strings(entry.get("enabledMcpjsonServers")), ["figma"]);
}

#[test]
fn removing_a_disabled_project_file_server_clears_its_name_from_the_project() {
    let harness = Harness::new();
    write_project_arrays(&harness.tree);
    let servers = harness.assemble(&harness.sample().build());

    harness
        .store
        .remove(&server(&servers, &format!("project:{PROJECT}/legacy")))
        .expect("removes the server");

    let entry = project_entry(&harness.tree);
    assert!(
        strings(entry.get("disabledMcpjsonServers")).is_empty(),
        "the removed server's name is still listed as disabled, so a server added under it next would come back disabled"
    );
    assert_eq!(strings(entry.get("enabledMcpjsonServers")), ["figma"]);
}

#[test]
fn renaming_a_project_file_server_does_not_leave_the_old_name_behind() {
    let harness = Harness::new();
    write_project_arrays(&harness.tree);
    let servers = harness.assemble(&harness.sample().build());

    harness
        .store
        .edit(
            &server(&servers, &format!("project:{PROJECT}/legacy")),
            "legacy-v2",
            &config(json!({"command": "newer-server"})),
        )
        .expect("edits the server");

    let entry = project_entry(&harness.tree);
    assert_eq!(strings(entry.get("disabledMcpjsonServers")), ["legacy-v2"]);
    assert_eq!(strings(entry.get("enabledMcpjsonServers")), ["figma"]);
}

#[test]
fn edit_of_a_parked_server_rewrites_the_record_without_the_cli() {
    let harness = Harness::new();
    let live = harness.assemble(&harness.sample().build());
    harness
        .store
        .set_enabled(&server(&live, "user/context7"), false)
        .expect("parks the server");
    let parked = harness.assemble(&harness.sample().without_user_servers().build());

    harness
        .store
        .edit(
            &server(&parked, "user/context7"),
            "context7-next",
            &playwright(),
        )
        .expect("edits the record");

    assert_eq!(harness.calls().len(), 1);
    let records = harness.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], json!("user/context7-next"));
    assert_eq!(records[0]["name"], json!("context7-next"));
    assert_eq!(
        records[0]["config"],
        serde_json::to_value(playwright()).expect("serialize config")
    );
}

#[test]
fn a_failed_edit_puts_the_original_server_back() {
    let harness = Harness::with_cli(Arc::new(FakeMcpMutating::failing_adds(1)));
    let servers = harness.assemble(&harness.sample().build());

    let error = harness
        .store
        .edit(
            &server(&servers, "user/context7"),
            "context7-next",
            &playwright(),
        )
        .expect_err("the re-add fails");

    assert!(is_cli_failure(&error), "unexpected error {error:?}");
    let calls = harness.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(
        calls[0],
        Call::Remove {
            name: "context7".to_string(),
            scope: Scope::User,
            local: false
        }
    );
    let (attempted, _, _, _) = added(&calls[1]);
    assert_eq!(attempted, "context7-next");
    let (restored, config, scope, local) = added(&calls[2]);
    assert_eq!(restored, "context7");
    assert_eq!(scope, Scope::User);
    assert!(!local);
    assert_eq!(config, context7());
}

#[test]
fn an_edit_that_cannot_be_put_back_reports_the_original_config() {
    let harness = Harness::with_cli(Arc::new(FakeMcpMutating::failing_adds(usize::MAX)));
    let servers = harness.assemble(&harness.sample().build());

    let error = harness
        .store
        .edit(
            &server(&servers, "user/context7"),
            "context7-next",
            &playwright(),
        )
        .expect_err("the restore fails");

    let message = error.to_string();
    match error {
        CoreError::UnrestorableMcpServer { name, config_json } => {
            assert_eq!(name, "context7");
            assert_eq!(
                serde_json::from_str::<McpServerConfig>(&config_json).expect("decode"),
                context7()
            );
            assert!(message.contains(&config_json));
            assert!(message.contains("CONTEXT7_API_KEY"));
        }
        other => panic!("expected an unrestorable-server error, got {other:?}"),
    }
}

#[test]
fn edit_refuses_to_rename_onto_an_existing_name() {
    let harness = Harness::new();
    let servers = harness.assemble(&harness.sample().build());

    let error = harness
        .store
        .edit(
            &server(&servers, &format!("project:{PROJECT}/figma")),
            "fresh",
            &figma(),
        )
        .expect_err("refuses the rename");

    match error {
        CoreError::DuplicateMcpName { name } => assert_eq!(name, "fresh"),
        other => panic!("expected a duplicate-name error, got {other:?}"),
    }
    assert!(harness.calls().is_empty());
}

#[test]
fn remove_calls_the_cli_at_the_origin_scope() {
    let harness = Harness::new();
    let servers = harness.assemble(&harness.sample().build());

    harness
        .store
        .remove(&server(&servers, &format!("local:{PROJECT}/playwright")))
        .expect("removes the server");

    assert_eq!(
        harness.calls(),
        [Call::Remove {
            name: "playwright".to_string(),
            scope: Scope::Project {
                path: PROJECT.to_string()
            },
            local: true
        }]
    );
}

#[test]
fn remove_of_a_parked_server_only_drops_the_record() {
    let harness = Harness::new();
    let live = harness.assemble(&harness.sample().build());
    harness
        .store
        .set_enabled(&server(&live, "user/context7"), false)
        .expect("parks the server");
    let parked = harness.assemble(&harness.sample().without_user_servers().build());

    harness
        .store
        .remove(&server(&parked, "user/context7"))
        .expect("drops the record");

    assert_eq!(harness.calls().len(), 1);
    assert!(harness.records().is_empty());
}

#[test]
fn a_corrupt_disabled_store_is_copied_aside_rebuilt_and_surfaced() {
    let harness = Harness::new();
    let garbage = "{ not the store you are looking for";
    harness.tree.write(garbage, &harness.disabled_json());

    let error = harness
        .store
        .assemble(&harness.sample().build(), &[])
        .expect_err("refuses the corrupt store");

    match error {
        CoreError::CorruptStore {
            path,
            reason,
            backup,
        } => {
            assert_eq!(path, harness.disabled_json().display().to_string());
            assert!(!reason.is_empty());
            let backup = backup.expect("a backup path");
            assert!(
                Path::new(&backup)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("a file name")
                    .starts_with("mcp-disabled.json.corrupt-")
            );
            assert_eq!(
                std::fs::read_to_string(&backup).expect("read the backup"),
                garbage
            );
        }
        other => panic!("expected a corrupt-store error, got {other:?}"),
    }

    let rebuilt = harness.assemble(&harness.sample().build());
    assert!(rebuilt.iter().any(|server| server.id == "user/context7"));
}

#[test]
fn parking_two_servers_at_once_keeps_both_records() {
    let tree = TempTree::new();
    let base = Instant::now();
    let cli = Arc::new(FakeMcpMutating::new());
    let store = Arc::new(McpStore::with_clocks(
        &tree.paths,
        cli.clone(),
        Arc::new(SlowFirstClock::new(DISABLED_AT, Duration::from_millis(200))),
        Arc::new(FakeMonotonicClock::new(base + Duration::from_secs(1))),
    ));
    let config = DiscoveredConfig {
        user_mcp_servers: BTreeMap::from([
            ("context7".to_string(), context7()),
            ("playwright".to_string(), playwright()),
        ]),
        read_at: base + Duration::from_secs(2),
        ..Default::default()
    };
    let servers = store.assemble(&config, &[]).expect("assembles");

    let parkers: Vec<_> = ["user/context7", "user/playwright"]
        .into_iter()
        .map(|id| {
            let store = store.clone();
            let server = server(&servers, id);
            std::thread::spawn(move || store.set_enabled(&server, false).expect("parks the server"))
        })
        .collect();
    for parker in parkers {
        parker.join().expect("the parking thread finishes");
    }

    let raw = std::fs::read_to_string(tree.paths.mcp_disabled_file()).expect("read disabled store");
    let records: Vec<Value> = serde_json::from_str(&raw).expect("decode disabled store");
    let mut ids: Vec<&str> = records
        .iter()
        .map(|record| record["id"].as_str().expect("an id"))
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, ["user/context7", "user/playwright"]);
}

#[test]
fn promote_adds_at_user_scope_then_removes_the_local_one_in_that_order() {
    let harness = Harness::new();
    let servers = harness.assemble(&harness.sample().without_user_servers().build());

    harness
        .store
        .promote_to_user(&server(&servers, &format!("local:{PROJECT}/playwright")))
        .expect("promotes the server");

    let calls = harness.calls();
    assert_eq!(calls.len(), 2);
    let (name, config, scope, local) = added(&calls[0]);
    assert_eq!(name, "playwright");
    assert_eq!(scope, Scope::User);
    assert!(!local);
    assert_eq!(config, playwright());
    assert_eq!(
        calls[1],
        Call::Remove {
            name: "playwright".to_string(),
            scope: Scope::Project {
                path: PROJECT.to_string()
            },
            local: true
        }
    );
}

#[test]
fn promote_refuses_a_user_scope_origin() {
    let harness = Harness::new();
    let candidate = McpServer::new("context7", McpOrigin::UserScope, context7(), true);

    let error = harness
        .store
        .promote_to_user(&candidate)
        .expect_err("refuses the promotion");

    match error {
        CoreError::NotPromotable { origin } => assert_eq!(origin, "user"),
        other => panic!("expected a not-promotable error, got {other:?}"),
    }
    assert!(harness.calls().is_empty());
}

#[test]
fn renaming_a_disabled_project_file_server_lists_the_new_name_before_the_rename() {
    let harness = Harness::new();
    write_project_arrays(&harness.tree);
    let servers = harness.assemble(&harness.sample().build());

    harness
        .store
        .edit(
            &server(&servers, &format!("project:{PROJECT}/legacy")),
            "renamed",
            &legacy(),
        )
        .expect("renames the server");

    let entry = project_entry(&harness.tree);
    let disabled = strings(entry.get("disabledMcpjsonServers"));
    assert!(
        disabled.contains(&"renamed".to_string()),
        "a server switched off before the rename came back enabled after it: {disabled:?}"
    );
    assert!(!disabled.contains(&"legacy".to_string()));
}

#[test]
fn adding_a_server_clears_a_name_left_listed_as_disabled() {
    let harness = Harness::new();
    // "ghost" names no server: the sort of leftover an older build or Claude Code can strand.
    let mut projects = Map::new();
    projects.insert(
        PROJECT.to_string(),
        json!({"disabledMcpjsonServers": ["ghost"]}),
    );
    harness
        .tree
        .write_claude_json(json!({"projects": projects}));
    harness.assemble(&harness.sample().build());

    harness
        .store
        .add(
            "ghost",
            &fresh(),
            &McpOrigin::ProjectFile {
                project: PROJECT.to_string(),
            },
        )
        .expect("adds the server");

    let entry = project_entry(&harness.tree);
    assert!(
        !strings(entry.get("disabledMcpjsonServers")).contains(&"ghost".to_string()),
        "the new server landed switched off because an old name was still listed"
    );
}

#[test]
fn promote_refuses_a_disabled_local_server() {
    let harness = Harness::new();
    let servers = harness.assemble(&harness.sample().local_disabled(&["playwright"]).build());

    let error = harness
        .store
        .promote_to_user(&server(&servers, &format!("local:{PROJECT}/playwright")))
        .expect_err("refuses a server that is switched off");

    assert!(matches!(error, CoreError::PromoteDisabled { name } if name == "playwright"));
    assert!(harness.calls().is_empty());
}

#[test]
fn promote_refuses_a_project_file_origin() {
    let harness = Harness::new();
    let candidate = McpServer::new(
        "figma",
        McpOrigin::ProjectFile {
            project: PROJECT.to_string(),
        },
        figma(),
        true,
    );

    let error = harness
        .store
        .promote_to_user(&candidate)
        .expect_err("refuses the promotion");

    match error {
        CoreError::NotPromotable { origin } => assert_eq!(origin, format!("project:{PROJECT}")),
        other => panic!("expected a not-promotable error, got {other:?}"),
    }
    assert!(harness.calls().is_empty());
}

#[test]
fn promote_refuses_a_plugin_origin() {
    let harness = Harness::new();
    let candidate = McpServer::new(
        "accesslint",
        McpOrigin::Plugin {
            plugin_id: "accesslint@accesslint".to_string(),
        },
        playwright(),
        true,
    );

    let error = harness
        .store
        .promote_to_user(&candidate)
        .expect_err("refuses the promotion");

    match error {
        CoreError::NotPromotable { origin } => {
            assert_eq!(origin, "plugin:accesslint@accesslint")
        }
        other => panic!("expected a not-promotable error, got {other:?}"),
    }
    assert!(harness.calls().is_empty());
}

#[test]
fn promote_refuses_a_duplicate_name_already_at_user_scope() {
    let harness = Harness::new();
    harness.assemble(&harness.sample().build());
    let candidate = McpServer::new(
        "context7",
        McpOrigin::LocalScope {
            project: PROJECT.to_string(),
        },
        playwright(),
        true,
    );

    let error = harness
        .store
        .promote_to_user(&candidate)
        .expect_err("refuses the duplicate");

    match error {
        CoreError::DuplicateMcpName { name } => assert_eq!(name, "context7"),
        other => panic!("expected a duplicate-name error, got {other:?}"),
    }
    assert!(harness.calls().is_empty());
}

#[test]
fn a_failed_promote_add_never_attempts_the_remove() {
    let harness = Harness::with_cli(Arc::new(FakeMcpMutating::failing_adds(1)));
    let servers = harness.assemble(&harness.sample().without_user_servers().build());

    let error = harness
        .store
        .promote_to_user(&server(&servers, &format!("local:{PROJECT}/playwright")))
        .expect_err("the add fails");

    assert!(is_cli_failure(&error), "unexpected error {error:?}");
    let calls = harness.calls();
    assert_eq!(calls.len(), 1);
    assert!(matches!(calls[0], Call::Add { .. }));
}

#[test]
fn the_claude_cli_satisfies_the_mutating_trait() {
    let tree = TempTree::new();
    let binary = tree.root().join("bin").join("claude");
    tree.write("#!/bin/sh\nexit 0\n", &binary);
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).expect("chmod 0755");
    let runner = Arc::new(FakeCliRunner::new());
    let paths = Paths {
        binary_override: Some(binary),
        ..tree.paths.clone()
    };
    let cli: Arc<dyn McpMutating> = Arc::new(ClaudeCli::new(runner.clone(), &paths));

    cli.mcp_remove(
        "context7",
        &Scope::Project {
            path: PROJECT.to_string(),
        },
        true,
    )
    .expect("removes the server");

    let call = runner.last_call().expect("a recorded call");
    assert_eq!(
        call.args,
        [
            "mcp".to_string(),
            "remove".to_string(),
            "context7".to_string(),
            "-s".to_string(),
            "local".to_string(),
        ]
    );
    assert_eq!(call.cwd, Some(PathBuf::from(PROJECT)));
}

const SHARED: &str = "shared";

fn write_same_name_in_both_origins(tree: &TempTree, local_disabled: bool) {
    let mut projects = Map::new();
    projects.insert(
        PROJECT.to_string(),
        json!({
            "disabledMcpjsonServers": [SHARED],
            "disabledMcpServers": if local_disabled { json!([SHARED]) } else { json!([]) },
        }),
    );
    tree.write_claude_json(json!({"projects": projects}));
}

fn same_name_servers(harness: &Harness) -> Vec<McpServer> {
    let mut config = harness.sample().build();
    config.projects[0].local_mcp_servers = BTreeMap::from([(SHARED.to_string(), playwright())]);
    config.projects[0].disabled_mcpjson_servers = vec![SHARED.to_string()];
    config.projects[0].enabled_mcpjson_servers = Vec::new();
    config.project_mcp_json = BTreeMap::from([(
        PROJECT.to_string(),
        BTreeMap::from([(SHARED.to_string(), figma())]),
    )]);
    harness.assemble(&config)
}

fn local_id() -> String {
    format!("local:{PROJECT}/{SHARED}")
}

fn project_file_id() -> String {
    format!("project:{PROJECT}/{SHARED}")
}

#[test]
fn removing_a_local_server_keeps_a_same_named_project_file_server_disabled() {
    let harness = Harness::new();
    write_same_name_in_both_origins(&harness.tree, false);
    let servers = same_name_servers(&harness);

    harness
        .store
        .remove(&server(&servers, &local_id()))
        .expect("removes the local server");

    let entry = project_entry(&harness.tree);
    assert_eq!(strings(entry.get("disabledMcpjsonServers")), [SHARED]);
}

#[test]
fn renaming_a_local_server_keeps_a_same_named_project_file_server_disabled() {
    let harness = Harness::new();
    write_same_name_in_both_origins(&harness.tree, false);
    let servers = same_name_servers(&harness);

    harness
        .store
        .edit(
            &server(&servers, &local_id()),
            "renamed",
            &config(json!({"command": "newer-server"})),
        )
        .expect("renames the local server");

    let entry = project_entry(&harness.tree);
    assert_eq!(strings(entry.get("disabledMcpjsonServers")), [SHARED]);
}

#[test]
fn removing_a_project_file_server_keeps_a_same_named_local_server_disabled() {
    let harness = Harness::new();
    write_same_name_in_both_origins(&harness.tree, true);
    let servers = same_name_servers(&harness);

    harness
        .store
        .remove(&server(&servers, &project_file_id()))
        .expect("removes the project file server");

    let entry = project_entry(&harness.tree);
    assert_eq!(strings(entry.get("disabledMcpServers")), [SHARED]);
    assert!(strings(entry.get("disabledMcpjsonServers")).is_empty());
}

#[test]
fn renaming_a_project_file_server_keeps_a_same_named_local_server_disabled() {
    let harness = Harness::new();
    write_same_name_in_both_origins(&harness.tree, true);
    let servers = same_name_servers(&harness);

    harness
        .store
        .edit(
            &server(&servers, &project_file_id()),
            "renamed",
            &config(json!({"command": "newer-server"})),
        )
        .expect("renames the project file server");

    let entry = project_entry(&harness.tree);
    assert_eq!(strings(entry.get("disabledMcpServers")), [SHARED]);
}
