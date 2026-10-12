use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fettlr_core::testing::{FakeGitProbe, FakeRegistryClient};
use fettlr_core::{
    CoreError, GitProbing, MarketplaceRecord, McpOrigin, McpServer, McpServerConfig, PluginRecord,
    Provenance, Scope, Skill, UpdateChecker, UpdateState,
};

fn marketplace(name: &str, install_location: &str) -> MarketplaceRecord {
    MarketplaceRecord {
        name: name.to_string(),
        source: "github".to_string(),
        repo: Some(format!("acme/{name}")),
        url: None,
        install_location: install_location.to_string(),
    }
}

fn plugin(id: &str, version: &str) -> PluginRecord {
    let json = serde_json::json!({
        "id": id,
        "version": version,
        "scope": "user",
        "enabled": true,
        "installPath": format!("/fake/home/.claude/plugins/cache/{id}"),
    });
    serde_json::from_value(json).expect("decode plugin")
}

fn available_plugin(id: &str, version: &str) -> PluginRecord {
    let json = serde_json::json!({
        "id": id,
        "version": version,
        "scope": "user",
    });
    serde_json::from_value(json).expect("decode plugin")
}

fn skill(name: &str, provenance: Provenance, path: &str) -> Skill {
    Skill {
        id: format!("{}/{name}", Scope::User.key()),
        name: name.to_string(),
        title: None,
        summary: None,
        scope: Scope::User,
        provenance,
        path: PathBuf::from(path),
    }
}

fn mcp_server(name: &str, config: McpServerConfig) -> McpServer {
    McpServer::new(name, McpOrigin::UserScope, config, true)
}

fn stdio_config(command: &str, args: &[&str]) -> McpServerConfig {
    McpServerConfig {
        kind: None,
        command: Some(command.to_string()),
        args: Some(args.iter().map(|arg| arg.to_string()).collect()),
        env: None,
        url: None,
        headers: None,
        extra: Default::default(),
    }
}

fn http_config(url: &str) -> McpServerConfig {
    McpServerConfig {
        kind: Some("http".to_string()),
        command: None,
        args: None,
        env: None,
        url: Some(url.to_string()),
        headers: None,
        extra: Default::default(),
    }
}

fn default_checker() -> UpdateChecker {
    UpdateChecker::new(
        Arc::new(FakeGitProbe::new()),
        Arc::new(FakeRegistryClient::new()),
    )
}

struct CountingGitProbe {
    active: Mutex<usize>,
    peak: Mutex<usize>,
}

impl CountingGitProbe {
    fn new() -> Self {
        Self {
            active: Mutex::new(0),
            peak: Mutex::new(0),
        }
    }

    fn peak(&self) -> usize {
        *self.peak.lock().expect("lock poisoned")
    }
}

impl GitProbing for CountingGitProbe {
    fn behind_count(&self, _repo: &Path) -> Result<u32, CoreError> {
        {
            let mut active = self.active.lock().expect("lock poisoned");
            *active += 1;
            let mut peak = self.peak.lock().expect("lock poisoned");
            *peak = (*peak).max(*active);
        }
        std::thread::sleep(Duration::from_millis(20));
        *self.active.lock().expect("lock poisoned") -= 1;
        Ok(0)
    }

    fn pull_ff_only(&self, _repo: &Path) -> Result<(), CoreError> {
        Ok(())
    }
}

#[test]
fn marketplaces_report_behind_counts_and_isolate_failures() {
    let git = FakeGitProbe::with_counts_and_failures(
        [("/repos/behind", 4), ("/repos/current", 0)],
        ["/repos/broken"],
    );
    let checker = UpdateChecker::new(Arc::new(git), Arc::new(FakeRegistryClient::new()));

    let marketplaces = vec![
        marketplace("behind", "/repos/behind"),
        marketplace("current", "/repos/current"),
        marketplace("broken", "/repos/broken"),
        marketplace("nowhere", ""),
    ];
    let report = checker.check(&marketplaces, &[], &[], &[], &[]);

    assert_eq!(
        report.marketplaces["behind"],
        UpdateState::Behind { commits: 4 }
    );
    assert_eq!(report.marketplaces["current"], UpdateState::UpToDate);
    assert_eq!(report.marketplaces["broken"], UpdateState::Unknown);
    assert_eq!(report.marketplaces["nowhere"], UpdateState::Unknown);
}

#[test]
fn plugins_compare_against_available_versions() {
    let installed = vec![
        plugin("outdated@acme", "1.0.0"),
        plugin("current@acme", "1.1.0"),
        plugin("ahead@acme", "2.0.0"),
        plugin("padded@acme", "1.2"),
        plugin("mystery@acme", "unknown"),
        plugin("orphan@acme", "1.0.0"),
    ];
    let available = vec![
        available_plugin("outdated@acme", "1.1.0"),
        available_plugin("current@acme", "1.1.0"),
        available_plugin("ahead@acme", "1.9.0"),
        available_plugin("padded@acme", "1.2.0"),
        available_plugin("mystery@acme", "1.2.0"),
    ];

    let report = default_checker().check(&[], &installed, &available, &[], &[]);

    assert_eq!(
        report.plugins["outdated@acme#user"],
        UpdateState::UpdateAvailable {
            installed: "1.0.0".to_string(),
            latest: "1.1.0".to_string()
        }
    );
    assert_eq!(report.plugins["current@acme#user"], UpdateState::UpToDate);
    assert_eq!(report.plugins["ahead@acme#user"], UpdateState::UpToDate);
    assert_eq!(report.plugins["padded@acme#user"], UpdateState::UpToDate);
    assert_eq!(
        report.plugins["mystery@acme#user"],
        UpdateState::UpdateAvailable {
            installed: "unknown".to_string(),
            latest: "1.2.0".to_string()
        }
    );
    assert_eq!(report.plugins["orphan@acme#user"], UpdateState::Unknown);
}

#[test]
fn merged_plugin_list_never_matches_a_record_against_itself() {
    let merged = vec![
        plugin("installed@acme", "1.0.0"),
        available_plugin("catalogue@acme", "1.0.0"),
    ];

    let report = default_checker().check(&[], &merged, &merged, &[], &[]);

    assert_eq!(report.plugins["catalogue@acme#user"], UpdateState::Unknown);
    assert_eq!(report.plugins["installed@acme#user"], UpdateState::Unknown);
}

#[test]
fn duplicate_catalogue_entries_use_the_highest_version() {
    let installed = vec![plugin("dup@acme", "1.5.0")];
    let ascending = vec![
        available_plugin("dup@acme", "1.0.0"),
        available_plugin("dup@acme", "2.0.0"),
    ];
    let descending = vec![
        available_plugin("dup@acme", "2.0.0"),
        available_plugin("dup@acme", "1.0.0"),
    ];

    for available in [ascending, descending] {
        let report = default_checker().check(&[], &installed, &available, &[], &[]);
        assert_eq!(
            report.plugins["dup@acme#user"],
            UpdateState::UpdateAvailable {
                installed: "1.5.0".to_string(),
                latest: "2.0.0".to_string()
            }
        );
    }
}

#[test]
fn skills_only_check_git_clones() {
    let cloned = skill("superpowers", Provenance::GitClone, "/skills/superpowers");
    let local = skill("notes", Provenance::Local, "/skills/notes");
    let from_plugin = skill(
        "review",
        Provenance::Plugin {
            plugin_id: "acme@acme".to_string(),
        },
        "/skills/review",
    );
    let git = FakeGitProbe::with_counts([("/skills/superpowers", 2)]);
    let checker = UpdateChecker::new(Arc::new(git), Arc::new(FakeRegistryClient::new()));

    let report = checker.check(&[], &[], &[], &[cloned, local, from_plugin], &[]);

    assert_eq!(
        report.skills["user/superpowers"],
        UpdateState::Behind { commits: 2 }
    );
    assert_eq!(report.skills["user/notes"], UpdateState::Unknown);
    assert_eq!(report.skills["user/review"], UpdateState::Unknown);
}

#[test]
fn mcp_packages_compare_against_the_registry() {
    let registry = FakeRegistryClient::with_versions([
        ("npm:@upstash/context7-mcp", "1.4.0"),
        ("npm:pinned-current", "2.1.0"),
        ("pypi:mcp-server-fetch", "1.2.0"),
    ]);
    let checker = UpdateChecker::new(Arc::new(FakeGitProbe::new()), Arc::new(registry));

    let servers = vec![
        mcp_server(
            "stale",
            stdio_config("npx", &["-y", "pinned-current@2.0.0"]),
        ),
        mcp_server(
            "current",
            stdio_config("npx", &["-y", "pinned-current@2.1.0"]),
        ),
        mcp_server("fetch", stdio_config("uvx", &["mcp-server-fetch==1.0"])),
        mcp_server(
            "floating",
            stdio_config("npx", &["-y", "@upstash/context7-mcp"]),
        ),
        mcp_server(
            "tagged",
            stdio_config("npx", &["-y", "pinned-current@next"]),
        ),
        mcp_server("remote", http_config("https://mcp.example.com/mcp")),
        mcp_server("script", stdio_config("node", &["server.js"])),
        mcp_server("missing", stdio_config("npx", &["-y", "gone@1.0.0"])),
    ];

    let report = checker.check(&[], &[], &[], &[], &servers);

    assert_eq!(
        report.mcp_servers["user/stale"],
        UpdateState::UpdateAvailable {
            installed: "2.0.0".to_string(),
            latest: "2.1.0".to_string()
        }
    );
    assert_eq!(report.mcp_servers["user/current"], UpdateState::UpToDate);
    assert_eq!(
        report.mcp_servers["user/fetch"],
        UpdateState::UpdateAvailable {
            installed: "1.0".to_string(),
            latest: "1.2.0".to_string()
        }
    );
    assert_eq!(
        report.mcp_servers["user/floating"],
        UpdateState::TracksLatest
    );
    assert_eq!(report.mcp_servers["user/tagged"], UpdateState::TracksLatest);
    assert_eq!(report.mcp_servers["user/remote"], UpdateState::Unknown);
    assert_eq!(report.mcp_servers["user/script"], UpdateState::Unknown);
    assert_eq!(report.mcp_servers["user/missing"], UpdateState::Unknown);
}

#[test]
fn slow_registry_calls_fall_back_to_unknown() {
    let registry =
        FakeRegistryClient::with_versions_and_hanging([("npm:quick", "1.0.0")], ["npm:slow"]);
    let checker = UpdateChecker::with_registry_timeout(
        Arc::new(FakeGitProbe::new()),
        Arc::new(registry),
        Duration::from_millis(50),
    );

    let servers = vec![
        mcp_server("slow", stdio_config("npx", &["-y", "slow@0.9.0"])),
        mcp_server("quick", stdio_config("npx", &["-y", "quick@1.0.0"])),
    ];
    let report = checker.check(&[], &[], &[], &[], &servers);

    assert_eq!(report.mcp_servers["user/slow"], UpdateState::Unknown);
    assert_eq!(report.mcp_servers["user/quick"], UpdateState::UpToDate);
}

#[test]
fn empty_input_produces_an_empty_report() {
    let report = default_checker().check(&[], &[], &[], &[], &[]);

    assert!(report.marketplaces.is_empty());
    assert!(report.plugins.is_empty());
    assert!(report.skills.is_empty());
    assert!(report.mcp_servers.is_empty());
    assert!(!report.checked_at.is_empty());
    assert!(report.checked_at.ends_with('Z'));
}

#[test]
fn a_panicking_job_is_isolated_as_unknown() {
    let git = FakeGitProbe::with_counts_and_panics([("/repos/behind", 4)], ["/repos/boom"]);
    let checker = UpdateChecker::new(Arc::new(git), Arc::new(FakeRegistryClient::new()));

    let marketplaces = vec![
        marketplace("behind", "/repos/behind"),
        marketplace("boom", "/repos/boom"),
        marketplace("current", "/repos/current"),
    ];
    let report = checker.check(&marketplaces, &[], &[], &[], &[]);

    assert_eq!(
        report.marketplaces["behind"],
        UpdateState::Behind { commits: 4 }
    );
    assert_eq!(report.marketplaces["boom"], UpdateState::Unknown);
    assert_eq!(report.marketplaces["current"], UpdateState::UpToDate);
}

#[test]
fn at_most_six_checks_run_concurrently() {
    let git = Arc::new(CountingGitProbe::new());
    let checker = UpdateChecker::new(git.clone(), Arc::new(FakeRegistryClient::new()));

    let marketplaces: Vec<MarketplaceRecord> = (0..20)
        .map(|index| marketplace(&format!("mp-{index}"), &format!("/repos/mp-{index}")))
        .collect();
    let report = checker.check(&marketplaces, &[], &[], &[], &[]);

    assert_eq!(report.marketplaces.len(), 20);
    let peak = git.peak();
    assert!(peak <= 6, "peak was {peak}");
    assert!(peak >= 2, "peak was {peak}");
}

fn versionless_plugin(id: &str) -> PluginRecord {
    let json = serde_json::json!({
        "id": id,
        "scope": "user",
    });
    serde_json::from_value(json).expect("decode plugin")
}

#[test]
fn stale_ids_name_every_list_the_report_calls_due() {
    let git = FakeGitProbe::with_counts([
        ("/repos/behind".to_string(), 4),
        ("/skills/behind".to_string(), 2),
    ]);
    let checker = UpdateChecker::new(Arc::new(git), Arc::new(FakeRegistryClient::new()));

    let marketplaces = vec![
        marketplace("behind", "/repos/behind"),
        marketplace("current", "/repos/current"),
    ];
    let plugins = vec![
        plugin("widgets@acme", "1.0.0"),
        plugin("gadgets@acme", "2.0.0"),
    ];
    let available = vec![
        available_plugin("widgets@acme", "2.0.0"),
        available_plugin("gadgets@acme", "2.0.0"),
    ];
    let skills = vec![
        skill("behind", Provenance::GitClone, "/skills/behind"),
        skill("current", Provenance::GitClone, "/skills/current"),
    ];

    let stale = checker
        .check(&marketplaces, &plugins, &available, &skills, &[])
        .stale_ids();

    assert_eq!(stale.marketplaces, vec!["behind".to_string()]);
    assert_eq!(stale.plugins, vec!["widgets@acme#user".to_string()]);
    assert_eq!(stale.skills, vec!["user/behind".to_string()]);
    assert!(stale.mcp_servers.is_empty());
}

/// The state `update_all` refuses to act on: listing it as due would promise work the run skips.
#[test]
fn stale_ids_leave_out_a_plugin_whose_catalogue_entry_names_no_version() {
    let plugins = vec![plugin("widgets@acme", "1.0.0")];
    let available = vec![versionless_plugin("widgets@acme")];

    let report = default_checker().check(&[], &plugins, &available, &[], &[]);

    assert_eq!(
        report.plugins["widgets@acme#user"],
        UpdateState::UpdateAvailable {
            installed: "1.0.0".to_string(),
            latest: "unknown".to_string()
        }
    );
    assert!(report.stale_ids().plugins.is_empty());
}

#[test]
fn stale_ids_include_an_mcp_server_the_registry_says_is_behind() {
    let checker = UpdateChecker::new(
        Arc::new(FakeGitProbe::new()),
        Arc::new(FakeRegistryClient::with_versions([("npm:slow", "2.0.0")])),
    );
    let servers = vec![mcp_server(
        "slow",
        stdio_config("npx", &["-y", "slow@1.0.0"]),
    )];

    let stale = checker.check(&[], &[], &[], &[], &servers).stale_ids();

    assert_eq!(stale.mcp_servers, vec!["user/slow".to_string()]);
}

#[test]
fn two_versions_that_cannot_be_ordered_are_unknown_rather_than_an_update() {
    let installed = vec![
        plugin("prerelease@acme", "2.0.0"),
        plugin("empty@acme", ""),
        plugin("prefixed@acme", "1.0.0"),
    ];
    let available = vec![
        available_plugin("prerelease@acme", "1.9.0-rc.1"),
        available_plugin("empty@acme", "1.0.0"),
        available_plugin("prefixed@acme", "v1.0.0"),
    ];

    let report = default_checker().check(&[], &installed, &available, &[], &[]);

    assert_eq!(report.plugins["prerelease@acme#user"], UpdateState::Unknown);
    assert_eq!(report.plugins["empty@acme#user"], UpdateState::Unknown);
    // A leading v is the same release, not an update, and not an unknown either.
    assert_eq!(report.plugins["prefixed@acme#user"], UpdateState::UpToDate);
}

#[test]
fn a_directory_sourced_marketplace_is_never_probed() {
    let mut local = marketplace("local", "/repos/local");
    local.source = "directory".to_string();
    let mut plain_git = marketplace("plain", "/repos/plain");
    plain_git.source = "git".to_string();
    let git = FakeGitProbe::with_counts([("/repos/local", 4), ("/repos/plain", 1)]);
    let checker = UpdateChecker::new(Arc::new(git), Arc::new(FakeRegistryClient::new()));

    let report = checker.check(&[local, plain_git], &[], &[], &[], &[]);

    assert_eq!(report.marketplaces["local"], UpdateState::Unknown);
    assert_eq!(
        report.marketplaces["plain"],
        UpdateState::Behind { commits: 1 }
    );
}

#[test]
fn mcp_pins_ahead_of_or_beside_the_registry_are_not_updates() {
    let registry = FakeRegistryClient::with_versions([
        ("npm:ranged", "2.5.0"),
        ("npm:ahead", "1.9.3"),
        ("npm:odd", "1.9.3"),
    ]);
    let checker = UpdateChecker::new(Arc::new(FakeGitProbe::new()), Arc::new(registry));
    let servers = vec![
        mcp_server("ranged", stdio_config("npx", &["-y", "ranged@2"])),
        mcp_server("ahead", stdio_config("npx", &["-y", "ahead@2.0.0"])),
        mcp_server("odd", stdio_config("npx", &["-y", "odd@2.0.0-rc.1"])),
    ];

    let report = checker.check(&[], &[], &[], &[], &servers);

    assert_eq!(report.mcp_servers["user/ranged"], UpdateState::TracksLatest);
    assert_eq!(report.mcp_servers["user/ahead"], UpdateState::UpToDate);
    assert_eq!(report.mcp_servers["user/odd"], UpdateState::Unknown);
}
