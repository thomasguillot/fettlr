use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fettlr_core::testing::{FakeGitProbe, FakeRegistryClient};
use fettlr_core::{
    CatalogueVersion, CoreError, GitProbing, MarketplaceRecord, McpOrigin, McpServer,
    McpServerConfig, PluginCatalogue, PluginRecord, Provenance, Scope, Skill, SkillsCliOrigin,
    UpdateChecker, UpdateState,
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

fn catalogue(entries: &[(&str, CatalogueVersion)]) -> PluginCatalogue {
    entries
        .iter()
        .map(|(id, version)| (id.to_string(), version.clone()))
        .collect()
}

fn versioned(version: &str) -> CatalogueVersion {
    CatalogueVersion::Version(version.to_string())
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

    fn fetch_source(
        &self,
        _url: &str,
        _reference: Option<&str>,
        _cache_repo: &Path,
    ) -> Result<String, CoreError> {
        Err(CoreError::NoUpstream {
            repo: String::new(),
        })
    }

    fn tree_sha(
        &self,
        _cache_repo: &Path,
        _commit: &str,
        _folder: &str,
    ) -> Result<Option<String>, CoreError> {
        Err(CoreError::NoUpstream {
            repo: String::new(),
        })
    }

    fn export_folder(
        &self,
        _cache_repo: &Path,
        _commit: &str,
        _folder: &str,
        _destination: &Path,
    ) -> Result<(), CoreError> {
        Err(CoreError::NoUpstream {
            repo: String::new(),
        })
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
    let report = checker.check(&marketplaces, &[], &PluginCatalogue::default(), &[], &[]);

    assert_eq!(
        report.marketplaces["behind"],
        UpdateState::Behind { commits: 4 }
    );
    assert_eq!(report.marketplaces["current"], UpdateState::UpToDate);
    assert_eq!(report.marketplaces["broken"], UpdateState::Unknown);
    assert_eq!(report.marketplaces["nowhere"], UpdateState::Unknown);
}

#[test]
fn plugins_compare_against_the_catalogue() {
    let installed = vec![
        plugin("outdated@acme", "1.0.0"),
        plugin("current@acme", "1.1.0"),
        plugin("ahead@acme", "2.0.0"),
        plugin("padded@acme", "1.2"),
        plugin("mystery@acme", "unknown"),
        plugin("orphan@acme", "1.0.0"),
    ];
    let available = catalogue(&[
        ("outdated@acme", versioned("1.1.0")),
        ("current@acme", versioned("1.1.0")),
        ("ahead@acme", versioned("1.9.0")),
        ("padded@acme", versioned("1.2.0")),
        ("mystery@acme", versioned("1.2.0")),
    ]);

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
fn a_plugin_installed_in_two_scopes_is_judged_in_both() {
    let user = plugin("newspack@newspack-devkit", "0.52.2");
    let project: PluginRecord = serde_json::from_value(serde_json::json!({
        "id": "newspack@newspack-devkit",
        "version": "0.52.2",
        "scope": "project",
        "projectPath": "/p",
        "enabled": true,
        "installPath": "/fake/home/.claude/plugins/cache/newspack",
    }))
    .expect("decode plugin");
    let available = catalogue(&[("newspack@newspack-devkit", versioned("0.80.0"))]);

    let report = default_checker().check(&[], &[user, project], &available, &[], &[]);

    let expected = UpdateState::UpdateAvailable {
        installed: "0.52.2".to_string(),
        latest: "0.80.0".to_string(),
    };
    assert_eq!(report.plugins["newspack@newspack-devkit#user"], expected);
    assert_eq!(
        report.plugins["newspack@newspack-devkit#project:/p"],
        expected
    );
}

#[test]
fn a_commit_versioned_plugin_compares_against_the_pinned_commit() {
    let sha = "9847f2aa1a15f25df35ed1fb7b4557dbb60cd651";
    let installed = vec![
        plugin("same@acme", "9847f2aa1a15"),
        plugin("behind@acme", "58267944f921"),
        plugin("semver@acme", "7.0.0"),
        plugin("upper@acme", "9847f2aa1a15"),
    ];
    let available = catalogue(&[
        ("same@acme", CatalogueVersion::Commit(sha.to_string())),
        ("behind@acme", CatalogueVersion::Commit(sha.to_string())),
        ("semver@acme", CatalogueVersion::Commit(sha.to_string())),
        ("upper@acme", CatalogueVersion::Commit(sha.to_uppercase())),
    ]);

    let report = default_checker().check(&[], &installed, &available, &[], &[]);

    assert_eq!(report.plugins["same@acme#user"], UpdateState::UpToDate);
    assert_eq!(
        report.plugins["behind@acme#user"],
        UpdateState::UpdateAvailable {
            installed: "58267944f921".to_string(),
            latest: "9847f2aa1a15".to_string()
        }
    );
    assert_eq!(report.plugins["semver@acme#user"], UpdateState::Unknown);
    assert_eq!(report.plugins["upper@acme#user"], UpdateState::UpToDate);
}

#[test]
fn an_untracked_entry_is_unknown() {
    let plugins = vec![plugin("widgets@acme", "1.0.0")];
    let available = catalogue(&[("widgets@acme", CatalogueVersion::Untracked)]);

    let report = default_checker().check(&[], &plugins, &available, &[], &[]);

    assert_eq!(report.plugins["widgets@acme#user"], UpdateState::Unknown);
    assert!(report.stale_ids().plugins.is_empty());
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

    let report = checker.check(
        &[],
        &[],
        &PluginCatalogue::default(),
        &[cloned, local, from_plugin],
        &[],
    );

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

    let report = checker.check(&[], &[], &PluginCatalogue::default(), &[], &servers);

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
    let report = checker.check(&[], &[], &PluginCatalogue::default(), &[], &servers);

    assert_eq!(report.mcp_servers["user/slow"], UpdateState::Unknown);
    assert_eq!(report.mcp_servers["user/quick"], UpdateState::UpToDate);
}

#[test]
fn empty_input_produces_an_empty_report() {
    let report = default_checker().check(&[], &[], &PluginCatalogue::default(), &[], &[]);

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
    let report = checker.check(&marketplaces, &[], &PluginCatalogue::default(), &[], &[]);

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
    let report = checker.check(&marketplaces, &[], &PluginCatalogue::default(), &[], &[]);

    assert_eq!(report.marketplaces.len(), 20);
    let peak = git.peak();
    assert!(peak <= 6, "peak was {peak}");
    assert!(peak >= 2, "peak was {peak}");
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
    let available = catalogue(&[
        ("widgets@acme", versioned("2.0.0")),
        ("gadgets@acme", versioned("2.0.0")),
    ]);
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

    let stale = checker
        .check(&[], &[], &PluginCatalogue::default(), &[], &servers)
        .stale_ids();

    assert_eq!(stale.mcp_servers, vec!["user/slow".to_string()]);
}

#[test]
fn two_versions_that_cannot_be_ordered_are_unknown_rather_than_an_update() {
    let installed = vec![
        plugin("prerelease@acme", "2.0.0"),
        plugin("empty@acme", ""),
        plugin("prefixed@acme", "1.0.0"),
    ];
    let available = catalogue(&[
        ("prerelease@acme", versioned("1.9.0-rc.1")),
        ("empty@acme", versioned("1.0.0")),
        ("prefixed@acme", versioned("v1.0.0")),
    ]);

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

    let report = checker.check(
        &[local, plain_git],
        &[],
        &PluginCatalogue::default(),
        &[],
        &[],
    );

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

    let report = checker.check(&[], &[], &PluginCatalogue::default(), &[], &servers);

    assert_eq!(report.mcp_servers["user/ranged"], UpdateState::TracksLatest);
    assert_eq!(report.mcp_servers["user/ahead"], UpdateState::UpToDate);
    assert_eq!(report.mcp_servers["user/odd"], UpdateState::Unknown);
}

const OLD_TREE: &str = "1111111111111111111111111111111111111111";
const NEW_TREE: &str = "2222222222222222222222222222222222222222";
const EMIL_URL: &str = "https://github.com/emilkowalski/skills.git";

fn cli_origin(source: &str, folder: &str, hash: &str) -> SkillsCliOrigin {
    SkillsCliOrigin {
        source: source.to_string(),
        source_type: "github".to_string(),
        source_url: Some(format!("https://github.com/{source}.git")),
        reference: None,
        folder: folder.to_string(),
        folder_hash: hash.to_string(),
    }
}

fn cli_skill(name: &str, source: &str, folder: &str, hash: &str) -> Skill {
    skill(
        name,
        Provenance::SkillsCli(cli_origin(source, folder, hash)),
        &format!("/agents/skills/{name}"),
    )
}

fn cli_skill_with(name: &str, origin: SkillsCliOrigin) -> Skill {
    skill(
        name,
        Provenance::SkillsCli(origin),
        &format!("/agents/skills/{name}"),
    )
}

fn source_checker(git: Arc<FakeGitProbe>) -> UpdateChecker {
    UpdateChecker::new(git, Arc::new(FakeRegistryClient::new()))
        .with_source_cache(PathBuf::from("/cache/skill-sources"))
}

fn check_skills(checker: &UpdateChecker, skills: &[Skill]) -> fettlr_core::UpdateReport {
    checker.check(&[], &[], &PluginCatalogue::default(), skills, &[])
}

#[test]
fn a_skills_cli_skill_matching_its_lock_hash_is_up_to_date() {
    let git = Arc::new(FakeGitProbe::with_sources([(
        EMIL_URL,
        "c0ffee",
        BTreeMap::from([("skills/motion", OLD_TREE)]),
    )]));
    let checker = source_checker(git);

    let report = check_skills(
        &checker,
        &[cli_skill(
            "motion",
            "emilkowalski/skills",
            "skills/motion",
            OLD_TREE,
        )],
    );

    assert_eq!(report.skills["user/motion"], UpdateState::UpToDate);
}

#[test]
fn a_skills_cli_skill_whose_folder_changed_upstream_is_source_changed() {
    let git = Arc::new(FakeGitProbe::with_sources([(
        EMIL_URL,
        "c0ffee",
        BTreeMap::from([("skills/motion", NEW_TREE)]),
    )]));
    let checker = source_checker(git);

    let report = check_skills(
        &checker,
        &[cli_skill(
            "motion",
            "emilkowalski/skills",
            "skills/motion",
            OLD_TREE,
        )],
    );

    assert_eq!(report.skills["user/motion"], UpdateState::SourceChanged);
    assert!(
        report
            .stale_ids()
            .skills
            .contains(&"user/motion".to_string())
    );
}

#[test]
fn skills_from_one_source_share_one_fetch() {
    let git = Arc::new(FakeGitProbe::with_sources([(
        EMIL_URL,
        "c0ffee",
        BTreeMap::from([
            ("skills/a", NEW_TREE),
            ("skills/b", NEW_TREE),
            ("skills/c", OLD_TREE),
        ]),
    )]));
    let checker = source_checker(git.clone());

    let report = check_skills(
        &checker,
        &[
            cli_skill("a", "emilkowalski/skills", "skills/a", OLD_TREE),
            cli_skill("b", "emilkowalski/skills", "skills/b", OLD_TREE),
            cli_skill("c", "emilkowalski/skills", "skills/c", OLD_TREE),
        ],
    );

    assert_eq!(git.fetch_count(EMIL_URL), 1);
    assert_eq!(report.skills["user/a"], UpdateState::SourceChanged);
    assert_eq!(report.skills["user/b"], UpdateState::SourceChanged);
    assert_eq!(report.skills["user/c"], UpdateState::UpToDate);
}

#[test]
fn an_unreachable_source_leaves_only_its_skills_unknown() {
    let git = Arc::new(
        FakeGitProbe::with_sources([(
            EMIL_URL,
            "c0ffee",
            BTreeMap::from([("skills/motion", NEW_TREE)]),
        )])
        .with_unreachable_source("https://github.com/gone/skills.git"),
    );
    let checker = source_checker(git);

    let report = check_skills(
        &checker,
        &[
            cli_skill("lost", "gone/skills", "skills/lost", OLD_TREE),
            cli_skill("motion", "emilkowalski/skills", "skills/motion", OLD_TREE),
        ],
    );

    assert_eq!(report.skills["user/lost"], UpdateState::Unknown);
    assert_eq!(report.skills["user/motion"], UpdateState::SourceChanged);
}

#[test]
fn a_folder_gone_upstream_is_unknown() {
    let git = Arc::new(FakeGitProbe::with_sources([(
        EMIL_URL,
        "c0ffee",
        BTreeMap::from([("skills/other", NEW_TREE)]),
    )]));
    let checker = source_checker(git);

    let report = check_skills(
        &checker,
        &[cli_skill(
            "motion",
            "emilkowalski/skills",
            "skills/motion",
            OLD_TREE,
        )],
    );

    assert_eq!(report.skills["user/motion"], UpdateState::Unknown);
}

#[test]
fn a_non_github_or_hashless_entry_is_unknown() {
    let git = Arc::new(FakeGitProbe::with_sources([(
        EMIL_URL,
        "c0ffee",
        BTreeMap::from([("skills/a", NEW_TREE), ("skills/b", NEW_TREE)]),
    )]));
    let checker = source_checker(git.clone());

    let mut not_github = cli_origin("emilkowalski/skills", "skills/a", OLD_TREE);
    not_github.source_type = "git".to_string();
    let sha256 = "a".repeat(64);

    let report = check_skills(
        &checker,
        &[
            cli_skill_with("a", not_github),
            cli_skill("b", "emilkowalski/skills", "skills/b", &sha256),
        ],
    );

    assert_eq!(report.skills["user/a"], UpdateState::Unknown);
    assert_eq!(report.skills["user/b"], UpdateState::Unknown);
    assert_eq!(git.fetch_count(EMIL_URL), 0);
}

#[test]
fn a_non_https_source_url_is_unknown() {
    let git = Arc::new(FakeGitProbe::new());
    let checker = source_checker(git.clone());

    let urls = [
        Some("file:///etc/passwd"),
        Some("git@github.com:emilkowalski/skills.git"),
        Some("ssh://git@github.com/emilkowalski/skills.git"),
        Some("http://github.com/emilkowalski/skills.git"),
        None,
    ];
    let skills: Vec<Skill> = urls
        .iter()
        .enumerate()
        .map(|(index, url)| {
            let mut origin = cli_origin("emilkowalski/skills", "skills/a", OLD_TREE);
            origin.source_url = url.map(str::to_string);
            cli_skill_with(&format!("s{index}"), origin)
        })
        .collect();

    let report = check_skills(&checker, &skills);

    for index in 0..urls.len() {
        assert_eq!(
            report.skills[&format!("user/s{index}")],
            UpdateState::Unknown
        );
    }
    for url in urls.into_iter().flatten() {
        assert_eq!(git.fetch_count(url), 0);
    }
}

#[test]
fn without_a_source_cache_skills_cli_skills_are_unknown() {
    let git = Arc::new(FakeGitProbe::with_sources([(
        EMIL_URL,
        "c0ffee",
        BTreeMap::from([("skills/motion", NEW_TREE)]),
    )]));
    let checker = UpdateChecker::new(git.clone(), Arc::new(FakeRegistryClient::new()));

    let report = check_skills(
        &checker,
        &[cli_skill(
            "motion",
            "emilkowalski/skills",
            "skills/motion",
            OLD_TREE,
        )],
    );

    assert_eq!(report.skills["user/motion"], UpdateState::Unknown);
    assert_eq!(git.fetch_count(EMIL_URL), 0);
}

#[test]
fn a_skill_with_a_reference_fetches_that_reference() {
    let git = Arc::new(FakeGitProbe::with_sources([(
        EMIL_URL,
        "c0ffee",
        BTreeMap::from([("skills/motion", OLD_TREE)]),
    )]));
    let checker = source_checker(git.clone());
    let mut origin = cli_origin("emilkowalski/skills", "skills/motion", OLD_TREE);
    origin.reference = Some("next".to_string());

    let report = check_skills(&checker, &[cli_skill_with("motion", origin)]);

    assert_eq!(report.skills["user/motion"], UpdateState::UpToDate);
    assert_eq!(git.fetched_references(), vec![Some("next".to_string())]);
}

#[test]
fn a_skills_cli_folder_that_climbs_out_of_the_source_is_unknown_without_a_fetch() {
    let git = Arc::new(FakeGitProbe::with_sources([(
        EMIL_URL,
        "c0ffee",
        BTreeMap::from([("../../evil", NEW_TREE)]),
    )]));
    let checker = source_checker(git.clone());

    let report = check_skills(
        &checker,
        &[cli_skill(
            "evil",
            "emilkowalski/skills",
            "../../evil",
            OLD_TREE,
        )],
    );

    assert_eq!(report.skills["user/evil"], UpdateState::Unknown);
    assert_eq!(git.fetch_count(EMIL_URL), 0);
}

#[test]
fn an_account_skill_is_never_checked() {
    let account = skill(
        "impeccable",
        Provenance::Account {
            updated_at: "2026-10-24T12:57:30Z".to_string(),
        },
        "/skills/synced/b1/impeccable",
    );
    let git = Arc::new(FakeGitProbe::with_counts([(
        "/skills/synced/b1/impeccable",
        4,
    )]));
    let checker = UpdateChecker::new(git.clone(), Arc::new(FakeRegistryClient::new()));

    let report = check_skills(&checker, &[account]);

    assert_eq!(report.skills["user/impeccable"], UpdateState::Unknown);
    assert!(git.fetched_references().is_empty());
}
