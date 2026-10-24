use fettlr_core::{
    ConnectorState, CoreError, McpListing, McpServerConfig, Scope, UNKNOWN_VERSION,
    merge_connectors, parse_marketplace_list, parse_mcp_list, parse_plugin_catalogue,
    parse_plugin_list,
};

const PLUGIN_LIST: &str = include_str!("fixtures/plugin-list.json");
const PLUGIN_LIST_AVAILABLE: &str = include_str!("fixtures/plugin-list-available.json");
const MARKETPLACE_LIST: &str = include_str!("fixtures/marketplace-list.json");

fn installed_part_of(json: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(json).expect("valid fixture");
    serde_json::to_string(&value["installed"]).expect("serializable")
}

#[test]
fn plugin_list_decodes_every_entry() {
    let plugins = parse_plugin_list(PLUGIN_LIST).expect("valid fixture");
    assert_eq!(plugins.len(), 4);

    let first = &plugins[0];
    assert_eq!(first.plugin_id, "accesslint@accesslint");
    assert_eq!(first.name, "accesslint");
    assert_eq!(first.marketplace_name, "accesslint");
    assert_eq!(first.version, "0.10.2");
    assert!(first.enabled);
    assert_eq!(
        first.install_path,
        "/Users/dev/.claude/plugins/cache/accesslint/accesslint/0.10.2"
    );
    assert!(!first.is_available_only);
    assert_eq!(first.description, None);
    assert_eq!(
        first.installed_at.as_deref(),
        Some("2026-03-28T17:16:40.547Z")
    );
    assert_eq!(
        first.last_updated.as_deref(),
        Some("2026-08-19T07:59:43.325Z")
    );
}

#[test]
fn commit_versions_decode_verbatim() {
    let plugins = parse_plugin_list(PLUGIN_LIST).expect("valid fixture");
    assert_eq!(
        plugins
            .iter()
            .filter(|p| p.version == "58267944f921")
            .count(),
        3
    );
}

#[test]
fn a_record_with_no_version_reads_as_the_unknown_sentinel() {
    let plugins =
        parse_plugin_list(r#"[{"id":"widgets@acme","installPath":"/p"}]"#).expect("valid json");
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].version, UNKNOWN_VERSION);
}

#[test]
fn disabled_plugin_decodes_as_disabled() {
    let plugins = parse_plugin_list(PLUGIN_LIST).expect("valid fixture");
    let linear = plugins
        .iter()
        .find(|p| p.name == "linear")
        .expect("linear present");
    assert!(!linear.enabled);
}

#[test]
fn project_scope_carries_project_path() {
    let plugins = parse_plugin_list(PLUGIN_LIST).expect("valid fixture");
    let scoped = plugins
        .iter()
        .find(|p| p.scope.project_path().is_some())
        .expect("a project-scoped plugin exists");
    assert_eq!(
        scoped.scope,
        Scope::Project {
            path: "/Users/dev/Sites/example-project".to_string()
        }
    );
    assert_eq!(
        scoped.scope.key(),
        "project:/Users/dev/Sites/example-project"
    );
    assert_eq!(scoped.scope.cli_value(), Some("project"));
}

#[test]
fn user_scope_has_no_project_path() {
    let plugins = parse_plugin_list(PLUGIN_LIST).expect("valid fixture");
    let user = plugins
        .iter()
        .find(|p| p.plugin_id == "accesslint@accesslint")
        .expect("accesslint present");
    assert_eq!(user.scope, Scope::User);
    assert_eq!(user.scope.project_path(), None);
    assert_eq!(user.scope.key(), "user");
    assert_eq!(user.scope.cli_value(), Some("user"));
}

#[test]
fn same_plugin_in_two_scopes_gets_distinct_identifiers() {
    let plugins = parse_plugin_list(PLUGIN_LIST).expect("valid fixture");
    let context7: Vec<_> = plugins
        .iter()
        .filter(|p| p.plugin_id == "context7@claude-plugins-official")
        .collect();
    assert_eq!(context7.len(), 2);

    let ids: std::collections::BTreeSet<_> = context7.iter().map(|p| p.id()).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains("context7@claude-plugins-official#user"));
    assert!(
        ids.contains("context7@claude-plugins-official#project:/Users/dev/Sites/example-project")
    );
}

fn decoded_scope(scope: Option<&str>, project_path: Option<&str>) -> Scope {
    let mut entry = serde_json::json!({ "id": "widgets@acme", "installPath": "/p" });
    if let Some(scope) = scope {
        entry["scope"] = serde_json::json!(scope);
    }
    if let Some(path) = project_path {
        entry["projectPath"] = serde_json::json!(path);
    }
    let list = serde_json::to_string(&serde_json::json!([entry])).expect("encodes");
    parse_plugin_list(&list).expect("decodes").remove(0).scope
}

#[test]
fn decoded_scope_strings_map_to_their_cases() {
    assert_eq!(decoded_scope(Some("user"), None), Scope::User);
    assert_eq!(decoded_scope(None, None), Scope::User);
    assert_eq!(
        decoded_scope(Some("project"), Some("/a")),
        Scope::Project {
            path: "/a".to_string()
        }
    );
    assert_eq!(
        decoded_scope(Some("local"), Some("/a")),
        Scope::Local {
            path: "/a".to_string()
        }
    );
    assert_eq!(
        decoded_scope(Some("synced"), None),
        Scope::Other {
            name: "synced".to_string()
        }
    );
    assert_eq!(
        decoded_scope(Some("something-new"), Some("/a")),
        Scope::Other {
            name: "something-new".to_string()
        }
    );
}

#[test]
fn local_and_user_installs_of_one_plugin_get_distinct_identifiers() {
    let list = r#"[
        {"id": "widgets@acme", "scope": "user", "installPath": "/p"},
        {"id": "widgets@acme", "scope": "local", "projectPath": "/a", "installPath": "/p"}
    ]"#;
    let plugins = parse_plugin_list(list).expect("decodes");
    let ids: Vec<_> = plugins.iter().map(|p| p.id()).collect();
    assert_eq!(ids, vec!["widgets@acme#user", "widgets@acme#local:/a"]);
}

#[test]
fn plugin_mcp_servers_decode() {
    let plugins = parse_plugin_list(PLUGIN_LIST).expect("valid fixture");
    let first = &plugins[0];
    let server = first
        .mcp_servers
        .as_ref()
        .and_then(|servers| servers.get("accesslint"))
        .expect("accesslint mcp server present");
    assert_eq!(server.kind, None);
    assert_eq!(server.command.as_deref(), Some("npx"));
    assert_eq!(
        server.args,
        Some(vec!["-y".to_string(), "@accesslint/mcp@latest".to_string()])
    );

    let linear = plugins
        .iter()
        .find(|p| p.name == "linear")
        .expect("linear present");
    let http = linear
        .mcp_servers
        .as_ref()
        .and_then(|servers| servers.get("linear"))
        .expect("linear mcp server present");
    assert_eq!(http.kind.as_deref(), Some("http"));
    assert_eq!(http.url.as_deref(), Some("https://mcp.linear.app/mcp"));
    assert_eq!(http.command, None);
}

#[test]
fn unknown_keys_are_ignored() {
    let json = r#"[
        {
            "id": "future@marketplace",
            "version": "1.0.0",
            "scope": "user",
            "enabled": true,
            "installPath": "/Users/dev/.claude/plugins/cache/marketplace/future/1.0.0",
            "installedAt": "2026-03-28T17:16:40.547Z",
            "lastUpdated": "2026-08-19T07:59:43.325Z",
            "somethingTheCLIAddedLater": { "nested": [1, 2, 3] },
            "anotherNewKey": true
        }
    ]"#;
    let plugins = parse_plugin_list(json).expect("unknown keys should be ignored");
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].plugin_id, "future@marketplace");
}

#[test]
fn malformed_plugin_json_returns_bad_json_error() {
    let result = parse_plugin_list("not valid json");
    assert!(matches!(result, Err(CoreError::BadJson(_))));
}

#[test]
fn malformed_marketplace_json_returns_bad_json_error() {
    let result = parse_marketplace_list("not valid json");
    assert!(matches!(result, Err(CoreError::BadJson(_))));
}

#[test]
fn available_listing_splits_installed_from_available() {
    let plugins = parse_plugin_list(PLUGIN_LIST_AVAILABLE).expect("valid fixture");
    assert_eq!(plugins.iter().filter(|p| !p.is_available_only).count(), 3);
    assert_eq!(plugins.iter().filter(|p| p.is_available_only).count(), 5);
}

#[test]
fn available_catalogue_can_repeat_an_installed_plugin() {
    let plugins = parse_plugin_list(PLUGIN_LIST_AVAILABLE).expect("valid fixture");
    let id = "linear@claude-plugins-official";
    assert!(
        plugins
            .iter()
            .any(|p| p.plugin_id == id && !p.is_available_only)
    );
    assert!(
        plugins
            .iter()
            .any(|p| p.plugin_id == id && p.is_available_only)
    );
}

#[test]
fn available_entries_use_plugin_id_key_and_carry_description() {
    let plugins = parse_plugin_list(PLUGIN_LIST_AVAILABLE).expect("valid fixture");
    let sdk = plugins
        .iter()
        .find(|p| p.name == "agent-sdk-dev")
        .expect("agent-sdk-dev present");
    assert_eq!(sdk.plugin_id, "agent-sdk-dev@claude-plugins-official");
    assert_eq!(sdk.marketplace_name, "claude-plugins-official");
    assert_eq!(
        sdk.description.as_deref(),
        Some("Development kit for working with the Claude Agent SDK")
    );
    assert!(sdk.install_path.is_empty());
    assert!(!sdk.enabled);
    assert_eq!(sdk.scope, Scope::User);
    assert_eq!(sdk.installed_at, None);
    assert_eq!(sdk.id(), "agent-sdk-dev@claude-plugins-official#user");
}

#[test]
fn available_entry_version_falls_back_to_unknown() {
    let plugins = parse_plugin_list(PLUGIN_LIST_AVAILABLE).expect("valid fixture");
    let sdk = plugins
        .iter()
        .find(|p| p.name == "agent-sdk-dev")
        .expect("agent-sdk-dev present");
    assert_eq!(sdk.version, "unknown");

    let design = plugins
        .iter()
        .find(|p| p.name == "design")
        .expect("design present");
    assert_eq!(design.version, "1.2.0");
}

#[test]
fn available_entry_tolerates_object_shaped_source() {
    let plugins = parse_plugin_list(PLUGIN_LIST_AVAILABLE).expect("valid fixture");
    assert!(plugins.iter().any(|p| p.name == "aws-serverless"));
    assert!(plugins.iter().any(|p| p.name == "notion"));
}

#[test]
fn plugin_catalogue_keeps_entries_the_merged_list_prunes() {
    let catalogue = parse_plugin_catalogue(PLUGIN_LIST_AVAILABLE).expect("valid fixture");
    assert_eq!(catalogue.len(), 5);
    assert!(catalogue.iter().all(|p| p.is_available_only));
    assert!(
        catalogue
            .iter()
            .any(|p| p.plugin_id == "linear@claude-plugins-official")
    );
}

#[test]
fn the_real_catalogue_leaves_out_what_is_installed_at_user_scope() {
    let installed = parse_plugin_list(&installed_part_of(PLUGIN_LIST_AVAILABLE)).unwrap();
    let catalogue = parse_plugin_catalogue(PLUGIN_LIST_AVAILABLE).unwrap();
    for record in installed.iter().filter(|r| r.scope == Scope::User) {
        assert!(
            !catalogue
                .iter()
                .any(|entry| entry.plugin_id == record.plugin_id)
        );
    }
    assert!(
        catalogue
            .iter()
            .any(|e| e.plugin_id == "linear@claude-plugins-official")
    );
}

#[test]
fn plugin_catalogue_falls_back_to_available_only_entries_for_a_flat_list() {
    let catalogue = parse_plugin_catalogue(PLUGIN_LIST).expect("valid fixture");
    assert!(catalogue.is_empty());
}

#[test]
fn marketplace_list_decodes_github_and_git_variants() {
    let markets = parse_marketplace_list(MARKETPLACE_LIST).expect("valid fixture");
    assert_eq!(markets.len(), 3);

    let github = markets
        .iter()
        .find(|m| m.name == "accesslint")
        .expect("accesslint present");
    assert_eq!(github.source, "github");
    assert_eq!(
        github.repo.as_deref(),
        Some("accesslint/claude-marketplace")
    );
    assert_eq!(github.url, None);
    assert_eq!(
        github.install_location,
        "/Users/dev/.claude/plugins/marketplaces/accesslint"
    );
    assert_eq!(github.display_source(), "accesslint/claude-marketplace");

    let git = markets
        .iter()
        .find(|m| m.name == "internal-devkit")
        .expect("internal-devkit present");
    assert_eq!(git.source, "git");
    assert_eq!(git.repo, None);
    assert_eq!(
        git.url.as_deref(),
        Some("https://git.example.com/acme/internal-devkit.git")
    );
    assert_eq!(
        git.display_source(),
        "https://git.example.com/acme/internal-devkit.git"
    );
}

#[test]
fn marketplace_display_source_falls_back_to_source() {
    let json =
        r#"[{ "name": "local", "source": "directory", "installLocation": "/Users/dev/mkt" }]"#;
    let markets = parse_marketplace_list(json).expect("valid json");
    assert_eq!(markets[0].display_source(), "directory");
}

#[test]
fn mcp_server_config_stdio_round_trips() {
    let json = r#"{
        "type": "stdio",
        "command": "npx",
        "args": ["-y", "@upstash/context7-mcp"],
        "env": { "CONTEXT7_API_KEY": "abc123" }
    }"#;
    let decoded: McpServerConfig = serde_json::from_str(json).expect("valid json");
    assert_eq!(decoded.kind.as_deref(), Some("stdio"));
    assert_eq!(decoded.command.as_deref(), Some("npx"));
    assert_eq!(
        decoded.args,
        Some(vec!["-y".to_string(), "@upstash/context7-mcp".to_string()])
    );
    let mut env = std::collections::BTreeMap::new();
    env.insert("CONTEXT7_API_KEY".to_string(), "abc123".to_string());
    assert_eq!(decoded.env, Some(env));
    assert_eq!(decoded.url, None);
    assert_eq!(decoded.headers, None);

    let reencoded = serde_json::to_string(&decoded).expect("serializes");
    let redecoded: McpServerConfig = serde_json::from_str(&reencoded).expect("round trips");
    assert_eq!(redecoded, decoded);
}

#[test]
fn mcp_server_config_http_round_trips() {
    let json = r#"{
        "type": "http",
        "url": "https://mcp.figma.com/mcp",
        "headers": { "X-Figma-Plugin-Bundle": "figma_prod@2_2_96" }
    }"#;
    let decoded: McpServerConfig = serde_json::from_str(json).expect("valid json");
    assert_eq!(decoded.kind.as_deref(), Some("http"));
    assert_eq!(decoded.url.as_deref(), Some("https://mcp.figma.com/mcp"));
    let mut headers = std::collections::BTreeMap::new();
    headers.insert(
        "X-Figma-Plugin-Bundle".to_string(),
        "figma_prod@2_2_96".to_string(),
    );
    assert_eq!(decoded.headers, Some(headers));
    assert_eq!(decoded.command, None);

    let reencoded = serde_json::to_string(&decoded).expect("serializes");
    let redecoded: McpServerConfig = serde_json::from_str(&reencoded).expect("round trips");
    assert_eq!(redecoded, decoded);
}

#[test]
fn mcp_server_config_absent_keys_do_not_encode_as_null() {
    let config = McpServerConfig {
        kind: None,
        command: Some("node".to_string()),
        args: Some(vec!["server.js".to_string()]),
        env: None,
        url: None,
        headers: None,
        extra: Default::default(),
    };
    let json = serde_json::to_string(&config).expect("serializes");
    assert!(!json.contains("url"));
    assert!(!json.contains("headers"));
    assert!(!json.contains("null"));
}

#[test]
fn an_object_that_is_not_a_plugin_listing_is_refused_rather_than_read_as_empty() {
    for json in [
        "{}",
        r#"{"error":"not logged in"}"#,
        r#"{"plugins":[{"id":"widgets@acme"}]}"#,
    ] {
        let error = parse_plugin_list(json).expect_err("refuses an unrecognised shape");
        assert!(
            matches!(error, CoreError::UnexpectedShape { .. }),
            "{json} produced {error:?}"
        );
        let error = parse_plugin_catalogue(json).expect_err("refuses an unrecognised shape");
        assert!(matches!(error, CoreError::UnexpectedShape { .. }));
    }
}

#[test]
fn a_grouped_listing_naming_only_one_side_is_still_read() {
    let installed = parse_plugin_list(r#"{"installed":[]}"#).expect("a known shape");
    assert!(installed.is_empty());
    let available = parse_plugin_list(r#"{"available":[]}"#).expect("a known shape");
    assert!(available.is_empty());
}

#[test]
fn a_key_fettlr_does_not_model_survives_a_round_trip() {
    let raw = r#"{"type":"stdio","command":"node","cwd":"/srv","envFile":".env","timeout":30}"#;
    let config: McpServerConfig = serde_json::from_str(raw).expect("decodes");

    assert_eq!(config.command.as_deref(), Some("node"));
    assert_eq!(config.extra.len(), 3);

    let encoded = serde_json::to_value(&config).expect("encodes");
    assert_eq!(
        encoded,
        serde_json::from_str::<serde_json::Value>(raw).unwrap()
    );
}

const MCP_LIST: &str = include_str!("fixtures/mcp-list.txt");

#[test]
fn mcp_list_separates_connectors_from_the_servers_fettlr_holds() {
    let rows = parse_mcp_list(MCP_LIST).expect("valid fixture");
    assert_eq!(rows.len(), 9);

    let connectors: Vec<&McpListing> = rows.iter().filter(|row| row.is_connector()).collect();
    assert_eq!(
        connectors
            .iter()
            .map(|row| row.connector_label())
            .collect::<Vec<_>>(),
        ["Notion", "Google Drive", "Linear"]
    );
    // A name with spaces, and a status that is not "Connected", both survive verbatim.
    assert_eq!(connectors[2].name, "claude.ai Linear");
    assert_eq!(connectors[2].target, "https://mcp.linear.app/mcp");
    assert_eq!(connectors[2].status, "✘ Failed to connect");

    // A command carrying spaces and its own dashes is not mistaken for the status.
    let chrome = rows
        .iter()
        .find(|row| row.name == "chrome-devtools")
        .expect("chrome row");
    assert!(chrome.target.ends_with("Google Chrome"));
    assert_eq!(chrome.status, "✔ Connected");

    assert!(
        !rows
            .iter()
            .any(|row| row.name.starts_with("plugin:") && row.is_connector())
    );
}

#[test]
fn an_mcp_list_that_says_nothing_is_configured_is_empty() {
    let output = "No MCP servers configured. Use `claude mcp add` to add a server.\n";
    assert!(parse_mcp_list(output).expect("empty list").is_empty());
}

#[test]
fn an_mcp_list_that_names_no_server_is_refused() {
    for output in [
        "",
        "Checking MCP server health…\n\n",
        "something else entirely",
    ] {
        let error = parse_mcp_list(output).expect_err("refuses an unrecognised shape");
        assert!(matches!(error, CoreError::UnexpectedShape { .. }));
    }
}

#[test]
fn a_connector_the_list_no_longer_serves_is_still_named() {
    let listed = parse_mcp_list(MCP_LIST).expect("valid fixture");
    let ever = vec![
        "claude.ai Notion".to_string(),
        "claude.ai Linear".to_string(),
        "claude.ai Granola".to_string(),
    ];

    let connectors = merge_connectors(&listed, &ever);

    assert_eq!(
        connectors
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>(),
        ["Google Drive", "Granola", "Linear", "Notion"]
    );

    let granola = &connectors[1];
    assert_eq!(granola.state, ConnectorState::Inactive);
    assert_eq!(granola.target, None, "there is nothing to point at");
    assert_eq!(granola.status, None);

    // The statuses the CLI printed are classified for a badge and kept verbatim for the reader.
    let linear = &connectors[2];
    assert_eq!(linear.state, ConnectorState::Failed);
    assert_eq!(linear.status.as_deref(), Some("✘ Failed to connect"));
    assert_eq!(connectors[3].state, ConnectorState::Connected);

    // A plugin server is not a connector, however the account happens to name things.
    assert!(!connectors.iter().any(|c| c.name.starts_with("plugin:")));
}
