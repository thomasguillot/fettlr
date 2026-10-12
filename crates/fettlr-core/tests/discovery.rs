use std::path::{Path, PathBuf};
use std::time::Instant;

use fettlr_core::testing::TempTree;
use fettlr_core::{
    CoreError, DiscoveredConfig, PluginRecord, Provenance, Scope, discover, plugin_skills,
};
use serde_json::{Map, Value, json};

struct Sample {
    tree: TempTree,
    project: PathBuf,
    absent_project: PathBuf,
    config: DiscoveredConfig,
}

fn make_sample() -> Sample {
    let tree = TempTree::new();
    let project = tree.make_project("example-project");
    let absent = tree.absent_project("gone-project");

    let mut projects = Map::new();
    projects.insert(
        project.display().to_string(),
        json!({
            "lastCost": 1.25,
            "allowedTools": [],
            "mcpServers": {
                "playwright": {"type": "stdio", "command": "npx", "args": ["@playwright/mcp"]}
            },
            "enabledMcpjsonServers": ["figma"],
            "disabledMcpjsonServers": ["legacy", "retired"],
        }),
    );
    projects.insert(
        absent.display().to_string(),
        json!({
            "mcpServers": {},
            "enabledMcpjsonServers": [],
            "disabledMcpjsonServers": ["ghost"],
        }),
    );

    tree.write_claude_json(json!({
        "numStartups": 42,
        "installMethod": "native",
        "oauthAccount": {"emailAddress": "dev@example.com"},
        "mcpServers": {
            "context-a8c": {
                "type": "stdio", "command": "npx", "args": ["-y", "@a8c/context"],
                "env": {"TOKEN": "abc123"},
            },
            "linear": {
                "type": "http", "url": "https://mcp.linear.app/mcp",
                "headers": {"X-Key": "secret"},
            },
        },
        "projects": Value::Object(projects),
    }));

    tree.make_skill(
        "alpha",
        &tree.user_skills_dir(),
        Some(&TempTree::skill_markdown("alpha", "The first skill.")),
        false,
    );
    tree.make_skill(
        "bravo-clone",
        &tree.user_skills_dir(),
        Some(&TempTree::skill_markdown("bravo-clone", "Cloned from git.")),
        true,
    );
    tree.make_skill("charlie", &tree.user_skills_dir(), None, false);
    tree.make_skill(
        ".hidden-skill",
        &tree.user_skills_dir(),
        Some(&TempTree::skill_markdown("hidden", "Should not appear.")),
        false,
    );
    tree.write("not a skill", &tree.user_skills_dir().join("notes.md"));

    let external = tree.make_skill(
        "delta",
        &tree.root().join("external"),
        Some(&TempTree::skill_markdown("delta", "Linked in.")),
        false,
    );
    tree.make_skill_symlink("delta-link", &tree.user_skills_dir(), &external);

    tree.make_skill(
        "echo",
        &tree.project_skills_dir(&project),
        Some(&TempTree::skill_markdown("echo", "Project scoped.")),
        false,
    );

    tree.write(
        r#"{
  "mcpServers": {
    "figma": { "type": "http", "url": "https://mcp.figma.com/mcp" },
    "shell": { "command": "node", "args": ["server.js"], "env": { "PORT": "9000" } }
  }
}"#,
        &project.join(".mcp.json"),
    );

    let config = discover(&tree.paths).expect("discovery succeeds");
    Sample {
        tree,
        project,
        absent_project: absent,
        config,
    }
}

#[test]
fn user_mcp_servers_decode_both_transports() {
    let sample = make_sample();
    assert_eq!(sample.config.user_mcp_servers.len(), 2);

    let stdio = sample
        .config
        .user_mcp_servers
        .get("context-a8c")
        .expect("context-a8c present");
    assert_eq!(stdio.kind.as_deref(), Some("stdio"));
    assert_eq!(stdio.command.as_deref(), Some("npx"));
    assert_eq!(
        stdio.args,
        Some(vec!["-y".to_string(), "@a8c/context".to_string()])
    );
    assert_eq!(
        stdio.env.as_ref().and_then(|env| env.get("TOKEN")),
        Some(&"abc123".to_string())
    );
    assert_eq!(stdio.url, None);

    let http = sample
        .config
        .user_mcp_servers
        .get("linear")
        .expect("linear present");
    assert_eq!(http.kind.as_deref(), Some("http"));
    assert_eq!(http.url.as_deref(), Some("https://mcp.linear.app/mcp"));
    assert_eq!(
        http.headers
            .as_ref()
            .and_then(|headers| headers.get("X-Key")),
        Some(&"secret".to_string())
    );
    assert_eq!(http.command, None);
}

#[test]
fn projects_carry_local_servers_and_mcpjson_arrays() {
    let sample = make_sample();
    assert_eq!(sample.config.projects.len(), 2);

    let project_path = sample.project.display().to_string();
    let live = sample
        .config
        .projects
        .iter()
        .find(|entry| entry.path == project_path)
        .expect("project present");
    assert!(live.exists);
    assert_eq!(live.local_mcp_servers.len(), 1);
    assert_eq!(
        live.local_mcp_servers
            .get("playwright")
            .and_then(|c| c.command.as_deref()),
        Some("npx")
    );
    assert_eq!(
        live.local_mcp_servers
            .get("playwright")
            .and_then(|c| c.args.clone()),
        Some(vec!["@playwright/mcp".to_string()])
    );
    assert_eq!(live.enabled_mcpjson_servers, vec!["figma".to_string()]);
    assert_eq!(
        live.disabled_mcpjson_servers,
        vec!["legacy".to_string(), "retired".to_string()]
    );
}

#[test]
fn absent_project_is_flagged_and_never_scanned() {
    let sample = make_sample();
    assert!(!sample.absent_project.exists());

    let absent_path = sample.absent_project.display().to_string();
    let gone = sample
        .config
        .projects
        .iter()
        .find(|entry| entry.path == absent_path)
        .expect("absent project present");
    assert!(!gone.exists);
    assert!(gone.local_mcp_servers.is_empty());
    assert_eq!(gone.disabled_mcpjson_servers, vec!["ghost".to_string()]);
    assert!(!sample.config.project_mcp_json.contains_key(&absent_path));

    let project_path = sample.project.display().to_string();
    assert!(sample.config.project_skills.iter().all(|skill| skill.scope
        == Scope::Project {
            path: project_path.clone()
        }));
}

#[test]
fn projects_are_sorted_by_path() {
    let sample = make_sample();
    let paths: Vec<&String> = sample.config.projects.iter().map(|p| &p.path).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
}

#[test]
fn user_skills_are_discovered_with_frontmatter() {
    let sample = make_sample();
    let names: Vec<&str> = sample
        .config
        .user_skills
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, vec!["alpha", "bravo-clone", "charlie", "delta-link"]);

    let alpha = sample
        .config
        .user_skills
        .iter()
        .find(|s| s.name == "alpha")
        .expect("alpha present");
    assert_eq!(alpha.id, "user/alpha");
    assert_eq!(alpha.title.as_deref(), Some("alpha"));
    assert_eq!(alpha.summary.as_deref(), Some("The first skill."));
    assert_eq!(alpha.scope, Scope::User);
    assert_eq!(alpha.provenance, Provenance::Local);
    assert_eq!(alpha.path, sample.tree.user_skills_dir().join("alpha"));
}

#[test]
fn git_clone_provenance_comes_from_a_dot_git_entry() {
    let sample = make_sample();
    let clone = sample
        .config
        .user_skills
        .iter()
        .find(|s| s.name == "bravo-clone")
        .expect("bravo-clone present");
    assert_eq!(clone.provenance, Provenance::GitClone);
    assert_eq!(clone.title.as_deref(), Some("bravo-clone"));
}

fn project_with_cloned_skill(trust: Option<bool>) -> DiscoveredConfig {
    let tree = TempTree::new();
    let project = tree.make_project("cloned-project");
    let mut entry = Map::new();
    if let Some(trust) = trust {
        entry.insert("hasTrustDialogAccepted".to_string(), json!(trust));
    }
    let mut projects = Map::new();
    projects.insert(project.display().to_string(), Value::Object(entry));
    tree.write_claude_json(json!({ "projects": Value::Object(projects) }));
    tree.make_skill(
        "cloned",
        &tree.project_skills_dir(&project),
        Some(&TempTree::skill_markdown("cloned", "A clone.")),
        true,
    );
    discover(&tree.paths).expect("discovery succeeds")
}

#[test]
fn a_git_cloned_skill_in_an_untrusted_project_is_local() {
    for trust in [Some(false), None] {
        let config = project_with_cloned_skill(trust);
        assert!(!config.projects[0].has_trust_dialog_accepted);
        assert_eq!(config.project_skills[0].provenance, Provenance::Local);
    }
}

#[test]
fn a_git_cloned_skill_in_a_trusted_project_stays_a_clone() {
    let config = project_with_cloned_skill(Some(true));
    assert!(config.projects[0].has_trust_dialog_accepted);
    assert_eq!(config.project_skills[0].provenance, Provenance::GitClone);
}

#[test]
fn trust_accepted_on_an_ancestor_folder_covers_its_projects_but_not_a_prefix_sibling() {
    let tree = TempTree::new();
    let parent = tree.make_project("work");
    let child = tree.make_project("work/app");
    let sibling = tree.make_project("workshop");
    tree.write_claude_json(json!({ "projects": {
        parent.display().to_string(): { "hasTrustDialogAccepted": true },
        child.display().to_string(): {},
        sibling.display().to_string(): {},
    } }));
    for project in [&child, &sibling] {
        tree.make_skill(
            "cloned",
            &tree.project_skills_dir(project),
            Some(&TempTree::skill_markdown("cloned", "A clone.")),
            true,
        );
    }
    let config = discover(&tree.paths).expect("discovery succeeds");
    let trusted = |path: &Path| {
        config
            .projects
            .iter()
            .find(|p| p.path == path.display().to_string())
            .expect("project present")
            .has_trust_dialog_accepted
    };
    assert!(trusted(&child));
    assert!(!trusted(&sibling));
    let provenance = |path: &Path| {
        config
            .project_skills
            .iter()
            .find(|s| matches!(&s.scope, Scope::Project { path: p } if *p == path.display().to_string()))
            .expect("skill present")
            .provenance
            .clone()
    };
    assert_eq!(provenance(&child), Provenance::GitClone);
    assert_eq!(provenance(&sibling), Provenance::Local);
}

#[test]
fn skill_without_markdown_is_still_listed() {
    let sample = make_sample();
    let charlie = sample
        .config
        .user_skills
        .iter()
        .find(|s| s.name == "charlie")
        .expect("charlie present");
    assert_eq!(charlie.title, None);
    assert_eq!(charlie.summary, None);
    assert_eq!(charlie.provenance, Provenance::Local);
}

#[test]
fn symlinked_skill_folder_is_discovered() {
    let sample = make_sample();
    let linked = sample
        .config
        .user_skills
        .iter()
        .find(|s| s.name == "delta-link")
        .expect("delta-link present");
    assert_eq!(linked.title.as_deref(), Some("delta"));
    assert_eq!(linked.summary.as_deref(), Some("Linked in."));
    assert_eq!(
        linked.path,
        sample.tree.user_skills_dir().join("delta-link")
    );
}

#[test]
fn hidden_folders_and_loose_files_are_skipped() {
    let sample = make_sample();
    assert!(
        !sample
            .config
            .user_skills
            .iter()
            .any(|s| s.name.starts_with('.'))
    );
    assert!(
        !sample
            .config
            .user_skills
            .iter()
            .any(|s| s.name == "notes.md")
    );
}

#[test]
fn project_skills_carry_project_scope() {
    let sample = make_sample();
    assert_eq!(sample.config.project_skills.len(), 1);
    let echo = &sample.config.project_skills[0];
    assert_eq!(echo.name, "echo");
    let project_path = sample.project.display().to_string();
    assert_eq!(
        echo.scope,
        Scope::Project {
            path: project_path.clone()
        }
    );
    assert_eq!(echo.id, format!("project:{project_path}/echo"));
    assert_eq!(echo.summary.as_deref(), Some("Project scoped."));
    assert_eq!(
        echo.path,
        sample.tree.project_skills_dir(&sample.project).join("echo")
    );
}

#[test]
fn project_mcp_json_is_read_per_project() {
    let sample = make_sample();
    assert_eq!(sample.config.project_mcp_json.len(), 1);
    let project_path = sample.project.display().to_string();
    let servers = sample
        .config
        .project_mcp_json
        .get(&project_path)
        .expect("project mcp.json present");
    assert_eq!(servers.len(), 2);
    assert_eq!(
        servers.get("figma").and_then(|s| s.url.as_deref()),
        Some("https://mcp.figma.com/mcp")
    );
    assert_eq!(
        servers.get("shell").and_then(|s| s.args.clone()),
        Some(vec!["server.js".to_string()])
    );
    assert_eq!(
        servers
            .get("shell")
            .and_then(|s| s.env.as_ref())
            .and_then(|env| env.get("PORT")),
        Some(&"9000".to_string())
    );
}

#[test]
fn a_well_formed_tree_produces_no_warnings() {
    let sample = make_sample();
    assert!(sample.config.warnings.is_empty());
}

#[test]
fn corrupt_root_config_errors() {
    let tree = TempTree::new();
    tree.write_claude_json_raw("{ \"mcpServers\": ");
    let error = discover(&tree.paths).expect_err("malformed JSON should error");
    assert!(
        matches!(&error, CoreError::MalformedRootConfig { path } if *path == tree.paths.claude_json.display().to_string())
    );
}

#[test]
fn root_config_that_is_not_an_object_errors() {
    let tree = TempTree::new();
    tree.write_claude_json_raw("[1, 2, 3]");
    let error = discover(&tree.paths).expect_err("non-object root should error");
    assert!(matches!(error, CoreError::MalformedRootConfig { .. }));
}

#[test]
fn unreadable_root_config_errors() {
    let tree = TempTree::new();
    tree.make_directory(&tree.paths.claude_json);
    let error = discover(&tree.paths).expect_err("a directory in place of the file should error");
    assert!(
        matches!(&error, CoreError::UnreadableRootConfig { path } if *path == tree.paths.claude_json.display().to_string())
    );
}

#[test]
fn absent_root_config_yields_an_empty_config() {
    let tree = TempTree::new();
    let config = discover(&tree.paths).expect("discovery succeeds");
    assert!(config.user_mcp_servers.is_empty());
    assert!(config.projects.is_empty());
    assert!(config.user_skills.is_empty());
    assert!(config.project_skills.is_empty());
    assert!(config.project_mcp_json.is_empty());
    assert!(config.warnings.is_empty());
}

#[test]
fn discover_stamps_the_read_from_the_monotonic_clock() {
    let tree = TempTree::new();
    tree.write_claude_json(json!({"mcpServers": {}}));

    let before = Instant::now();
    let config = discover(&tree.paths).expect("discovery succeeds");
    let after = Instant::now();

    assert!(config.read_at >= before);
    assert!(config.read_at <= after);
}

#[test]
fn corrupt_project_mcp_json_warns_and_skips() {
    let tree = TempTree::new();
    let project = tree.make_project("broken-project");
    let mut projects = Map::new();
    projects.insert(project.display().to_string(), json!({}));
    tree.write_claude_json(json!({ "projects": Value::Object(projects) }));
    tree.write("{ not json at all", &project.join(".mcp.json"));

    let config = discover(&tree.paths).expect("discovery succeeds");
    assert!(config.project_mcp_json.is_empty());
    assert_eq!(config.projects.len(), 1);
    assert_eq!(config.warnings.len(), 1);
    assert!(config.warnings[0].contains(&project.join(".mcp.json").display().to_string()));
}

#[test]
fn malformed_server_entries_are_skipped_with_a_warning() {
    let tree = TempTree::new();
    tree.write_claude_json_raw(
        r#"{
  "mcpServers": {
    "good": { "command": "node" },
    "text-instead-of-object": "nope",
    "wrong-types": { "command": "node", "args": [1, 2] }
  }
}"#,
    );

    let config = discover(&tree.paths).expect("discovery succeeds");
    let names: Vec<&String> = config.user_mcp_servers.keys().collect();
    assert_eq!(names, vec!["good"]);
    assert_eq!(config.warnings.len(), 2);
    assert!(
        config
            .warnings
            .iter()
            .any(|w| w.contains("text-instead-of-object"))
    );
    assert!(config.warnings.iter().any(|w| w.contains("wrong-types")));
}

#[test]
fn malformed_project_entry_is_skipped_with_a_warning() {
    let tree = TempTree::new();
    let good = tree.make_project("good-project");
    tree.write_claude_json_raw(&format!(
        r#"{{ "projects": {{ "{}": {{}}, "/Users/dev/oops": "not-an-object" }} }}"#,
        good.display()
    ));

    let config = discover(&tree.paths).expect("discovery succeeds");
    assert_eq!(
        config
            .projects
            .iter()
            .map(|p| p.path.clone())
            .collect::<Vec<_>>(),
        vec![good.display().to_string()]
    );
    assert_eq!(config.warnings.len(), 1);
    assert!(config.warnings[0].contains("/Users/dev/oops"));
}

#[test]
fn mcpjson_arrays_tolerate_non_string_entries() {
    let tree = TempTree::new();
    let project = tree.make_project("mixed-project");
    tree.write_claude_json_raw(&format!(
        r#"{{
  "projects": {{
    "{}": {{
      "enabledMcpjsonServers": ["figma", 7, null],
      "disabledMcpjsonServers": "not-an-array"
    }}
  }}
}}"#,
        project.display()
    ));

    let config = discover(&tree.paths).expect("discovery succeeds");
    let entry = &config.projects[0];
    assert_eq!(entry.enabled_mcpjson_servers, vec!["figma".to_string()]);
    assert!(entry.disabled_mcpjson_servers.is_empty());
}

#[test]
fn home_as_a_project_does_not_list_user_skills_twice() {
    let tree = TempTree::new();
    tree.make_skill(
        "alpha",
        &tree.user_skills_dir(),
        Some(&TempTree::skill_markdown("alpha", "Listed once.")),
        false,
    );
    let mut projects = Map::new();
    projects.insert(tree.root().display().to_string(), json!({}));
    tree.write_claude_json(json!({ "projects": Value::Object(projects) }));

    let config = discover(&tree.paths).expect("discovery succeeds");
    let names: Vec<&str> = config.user_skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["alpha"]);
    assert!(config.project_skills.is_empty());
    assert!(config.warnings.is_empty());
}

#[test]
fn skills_path_that_is_not_a_folder_is_ignored() {
    let tree = TempTree::new();
    tree.write("not a folder", &tree.paths.skills_dir());

    let config = discover(&tree.paths).expect("discovery succeeds");
    assert!(config.user_skills.is_empty());
    assert!(config.warnings.is_empty());
}

#[test]
fn every_discovered_path_stays_inside_the_injected_tree() {
    let sample = make_sample();
    let root = sample.tree.root().display().to_string();
    let mut paths: Vec<String> = sample
        .config
        .user_skills
        .iter()
        .map(|s| s.path.display().to_string())
        .collect();
    paths.extend(
        sample
            .config
            .project_skills
            .iter()
            .map(|s| s.path.display().to_string()),
    );
    paths.extend(sample.config.projects.iter().map(|p| p.path.clone()));
    assert!(!paths.is_empty());
    assert!(paths.iter().all(|p| p.starts_with(&root)));
}

mod plugin_provided_skills {
    use super::*;

    fn plugin_record(id: &str, install_path: &str, scope: &Scope, enabled: bool) -> PluginRecord {
        let mut object = json!({
            "id": id,
            "version": "1.0.0",
            "scope": scope.cli_value(),
            "enabled": enabled,
            "installPath": install_path,
        });
        if let Some(path) = scope.project_path() {
            object["projectPath"] = json!(path);
        }
        serde_json::from_value(object).expect("decode plugin record")
    }

    #[test]
    fn plugin_skills_are_discovered_from_the_install_path() {
        let tree = TempTree::new();
        tree.make_skill(
            "accessibility-audit",
            &tree.plugin_skills_dir("accesslint"),
            Some(&TempTree::skill_markdown(
                "accessibility-audit",
                "Audit a page.",
            )),
            false,
        );
        let record = plugin_record(
            "accesslint@accesslint",
            &tree.plugin_install_dir("accesslint").display().to_string(),
            &Scope::User,
            true,
        );

        let (skills, warnings) = plugin_skills(&[record]);

        assert!(warnings.is_empty());
        assert_eq!(skills.len(), 1);
        let skill = &skills[0];
        assert_eq!(skill.name, "accessibility-audit");
        assert_eq!(skill.title.as_deref(), Some("accessibility-audit"));
        assert_eq!(skill.summary.as_deref(), Some("Audit a page."));
        assert_eq!(
            skill.provenance,
            Provenance::Plugin {
                plugin_id: "accesslint@accesslint".to_string()
            }
        );
        assert_eq!(skill.scope, Scope::User);
        assert_eq!(skill.id, "plugin:accesslint@accesslint/accessibility-audit");
        assert_eq!(
            skill.path,
            tree.plugin_skills_dir("accesslint")
                .join("accessibility-audit")
        );
    }

    #[test]
    fn a_plugin_folder_without_skill_md_is_not_a_skill() {
        let tree = TempTree::new();
        let shared = tree.make_skill("shared", &tree.plugin_skills_dir("accesslint"), None, false);
        tree.write("notes", &shared.join("methodology.md"));
        let record = plugin_record(
            "accesslint@accesslint",
            &tree.plugin_install_dir("accesslint").display().to_string(),
            &Scope::User,
            true,
        );

        let (skills, warnings) = plugin_skills(&[record]);

        assert!(skills.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn plugin_skills_grouped_in_subfolders_are_discovered() {
        let tree = TempTree::new();
        let group = tree.plugin_skills_dir("mattpocock").join("engineering");
        tree.make_skill(
            "tdd",
            &group,
            Some(&TempTree::skill_markdown("tdd", "Test first.")),
            false,
        );
        let record = plugin_record(
            "mattpocock-skills@official",
            &tree.plugin_install_dir("mattpocock").display().to_string(),
            &Scope::User,
            true,
        );

        let (skills, _warnings) = plugin_skills(&[record]);

        assert_eq!(
            skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["tdd"]
        );
        assert_eq!(skills[0].id, "plugin:mattpocock-skills@official/tdd");
    }

    #[test]
    fn a_plugin_installed_in_two_scopes_lists_its_skills_once() {
        let tree = TempTree::new();
        let project = tree.make_project("example-project");
        tree.make_skill(
            "alpha",
            &tree.plugin_skills_dir("shared-plugin"),
            Some(&TempTree::skill_markdown("alpha", "Once only.")),
            false,
        );
        let install = tree
            .plugin_install_dir("shared-plugin")
            .display()
            .to_string();
        let records = vec![
            plugin_record(
                "shared@market",
                &install,
                &Scope::Project {
                    path: project.display().to_string(),
                },
                false,
            ),
            plugin_record("shared@market", &install, &Scope::User, true),
        ];

        let (skills, _warnings) = plugin_skills(&records);

        assert_eq!(skills.len(), 1);
    }

    #[test]
    fn an_available_only_plugin_contributes_no_skills() {
        let record = plugin_record("not-installed@market", "", &Scope::User, true);

        let (skills, warnings) = plugin_skills(&[record]);

        assert!(skills.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_plugin_skill_does_not_collide_with_a_user_skill_of_the_same_name() {
        let tree = TempTree::new();
        tree.make_skill(
            "review",
            &tree.user_skills_dir(),
            Some(&TempTree::skill_markdown("review", "Mine.")),
            false,
        );
        tree.make_skill(
            "review",
            &tree.plugin_skills_dir("reviewer"),
            Some(&TempTree::skill_markdown("review", "The plugin's.")),
            false,
        );
        let record = plugin_record(
            "reviewer@market",
            &tree.plugin_install_dir("reviewer").display().to_string(),
            &Scope::User,
            true,
        );

        let config = discover(&tree.paths).expect("discover succeeds");
        let (found, _warnings) = plugin_skills(&[record]);

        let mut ids: Vec<String> = config.user_skills.iter().map(|s| s.id.clone()).collect();
        ids.extend(found.iter().map(|s| s.id.clone()));

        assert_eq!(
            ids,
            vec![
                "user/review".to_string(),
                "plugin:reviewer@market/review".to_string()
            ]
        );
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), 2);
    }

    #[test]
    fn a_missing_plugin_skills_directory_is_tolerated_silently() {
        let tree = TempTree::new();
        tree.make_directory(&tree.plugin_install_dir("bare-plugin"));
        let record = plugin_record(
            "bare@market",
            &tree.plugin_install_dir("bare-plugin").display().to_string(),
            &Scope::User,
            true,
        );

        let (skills, warnings) = plugin_skills(&[record]);

        assert!(skills.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_disabled_plugins_skills_are_still_listed() {
        let tree = TempTree::new();
        tree.make_skill(
            "still-here",
            &tree.plugin_skills_dir("toggle"),
            Some(&TempTree::skill_markdown(
                "still-here",
                "Present regardless.",
            )),
            false,
        );
        let record = plugin_record(
            "toggle@market",
            &tree.plugin_install_dir("toggle").display().to_string(),
            &Scope::User,
            false,
        );

        let (skills, _warnings) = plugin_skills(&[record]);

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "still-here");
    }
}

mod plugin_provided_subagents {
    use super::*;
    use fettlr_core::plugin_subagents;

    fn plugin_record(id: &str, install_path: &str, scope: &Scope, enabled: bool) -> PluginRecord {
        let mut object = json!({
            "id": id,
            "version": "1.0.0",
            "scope": scope.cli_value(),
            "enabled": enabled,
            "installPath": install_path,
        });
        if let Some(path) = scope.project_path() {
            object["projectPath"] = json!(path);
        }
        serde_json::from_value(object).expect("decode plugin record")
    }

    fn installed(install: &Path) -> PluginRecord {
        plugin_record(
            "acme-widgets@shop",
            &install.display().to_string(),
            &Scope::User,
            true,
        )
    }

    #[test]
    fn reads_the_agents_folder_of_each_installed_plugin() {
        let tree = TempTree::new();
        let install = tree.plugin_install_dir("acme-widgets");
        tree.make_subagent(
            "auditor",
            &tree.plugin_agents_dir("acme-widgets"),
            Some(&TempTree::subagent_markdown("auditor", "Audits widgets.")),
        );
        let plugin = installed(&install);

        let (found, warnings) = plugin_subagents(std::slice::from_ref(&plugin));

        assert!(warnings.is_empty());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, format!("plugin:{}::auditor", plugin.plugin_id));
        assert_eq!(found[0].name, "auditor");
        assert_eq!(
            found[0].provenance,
            Provenance::Plugin {
                plugin_id: plugin.plugin_id.clone()
            }
        );
        assert_eq!(found[0].scope, plugin.scope);
        assert_eq!(
            found[0].path,
            tree.plugin_agents_dir("acme-widgets").join("auditor.md")
        );
    }

    #[test]
    fn a_manifest_agents_field_replaces_the_default_folder() {
        let tree = TempTree::new();
        let install = tree.plugin_install_dir("acme-widgets");
        tree.make_subagent("ignored", &tree.plugin_agents_dir("acme-widgets"), None);
        tree.make_subagent("from-dir", &install.join("custom-agents"), None);
        tree.make_subagent("single", &install.join("extra"), None);
        tree.write(
            r#"{ "name": "acme-widgets", "agents": ["./custom-agents/", "./extra/single.md"] }"#,
            &install.join(".claude-plugin").join("plugin.json"),
        );

        let (found, _) = plugin_subagents(&[installed(&install)]);

        let names: Vec<&str> = found.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["from-dir", "single"]);
    }

    #[test]
    fn a_manifest_agents_string_is_one_path() {
        let tree = TempTree::new();
        let install = tree.plugin_install_dir("acme-widgets");
        tree.make_subagent("only", &install.join("my-agents"), None);
        tree.write(
            r#"{ "agents": "./my-agents" }"#,
            &install.join(".claude-plugin").join("plugin.json"),
        );

        let (found, _) = plugin_subagents(&[installed(&install)]);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "only");
    }

    #[test]
    fn the_same_plugin_in_two_scopes_lists_each_subagent_once() {
        let tree = TempTree::new();
        let install = tree.plugin_install_dir("acme-widgets");
        tree.make_subagent("auditor", &tree.plugin_agents_dir("acme-widgets"), None);
        let user = installed(&install);
        let project = plugin_record(
            "acme-widgets@shop",
            &install.display().to_string(),
            &Scope::Project {
                path: "/tmp/somewhere".to_string(),
            },
            true,
        );

        let (found, _) = plugin_subagents(&[user, project]);

        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_plugin_without_an_install_path_contributes_nothing() {
        let plugin = plugin_record("acme-widgets@shop", "", &Scope::User, true);

        let (found, warnings) = plugin_subagents(&[plugin]);

        assert!(found.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn manifest_entries_outside_the_plugin_are_ignored_with_a_warning() {
        let tree = TempTree::new();
        let install = tree.plugin_install_dir("acme-widgets");
        let outside = tree.root().join("elsewhere");
        tree.make_subagent("escaped", &outside, None);
        tree.make_subagent("inside", &install.join("agents"), None);
        tree.write(
            &format!(
                r#"{{ "agents": ["{}", "../elsewhere", "./agents"] }}"#,
                outside.display()
            ),
            &install.join(".claude-plugin").join("plugin.json"),
        );

        let (found, warnings) = plugin_subagents(&[installed(&install)]);

        let names: Vec<&str> = found.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["inside"]);
        assert_eq!(warnings.len(), 2);
        assert!(
            warnings
                .iter()
                .all(|w| w.contains("points outside the plugin"))
        );
    }

    #[test]
    fn a_manifest_entry_naming_the_whole_plugin_is_ignored_with_a_warning() {
        let tree = TempTree::new();
        let install = tree.plugin_install_dir("acme-widgets");
        tree.make_subagent("inside", &tree.plugin_agents_dir("acme-widgets"), None);
        tree.write(
            r#"{ "agents": ["", ".", "./", "./agents"] }"#,
            &install.join(".claude-plugin").join("plugin.json"),
        );

        let (found, warnings) = plugin_subagents(&[installed(&install)]);

        let names: Vec<&str> = found.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["inside"]);
        assert_eq!(warnings.len(), 3);
        assert!(
            warnings
                .iter()
                .all(|w| w.contains("names the whole plugin"))
        );
    }
}

mod subagents {
    use super::*;

    #[test]
    fn user_subagents_are_discovered_with_their_frontmatter() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        tree.make_subagent(
            "reviewer",
            &tree.user_agents_dir(),
            Some(&TempTree::subagent_markdown("reviewer", "Reviews diffs.")),
        );

        let config = discover(&tree.paths).expect("discover");

        assert_eq!(config.user_subagents.len(), 1);
        let reviewer = &config.user_subagents[0];
        assert_eq!(reviewer.id, "user::reviewer");
        assert_eq!(reviewer.name, "reviewer");
        assert_eq!(reviewer.summary.as_deref(), Some("Reviews diffs."));
        assert_eq!(reviewer.model.as_deref(), Some("sonnet"));
        assert_eq!(reviewer.tools, vec!["Read", "Grep", "Bash"]);
        assert_eq!(reviewer.effort.as_deref(), Some("medium"));
        assert_eq!(reviewer.permission_mode.as_deref(), Some("default"));
        assert_eq!(reviewer.max_turns, Some(12));
        assert_eq!(reviewer.memory.as_deref(), Some("project"));
        assert_eq!(reviewer.scope, Scope::User);
        assert_eq!(reviewer.provenance, Provenance::Local);
        assert_eq!(reviewer.path, tree.user_agents_dir().join("reviewer.md"));
    }

    #[test]
    fn the_frontmatter_name_wins_over_the_file_stem() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        tree.make_subagent(
            "old-file-name",
            &tree.user_agents_dir(),
            Some(&TempTree::subagent_markdown(
                "security-reviewer",
                "Security.",
            )),
        );

        let config = discover(&tree.paths).expect("discover");

        assert_eq!(config.user_subagents[0].name, "security-reviewer");
        assert_eq!(config.user_subagents[0].id, "user::old-file-name");
    }

    #[test]
    fn a_file_without_frontmatter_is_listed_by_its_stem() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        tree.make_subagent("bare", &tree.user_agents_dir(), None);

        let config = discover(&tree.paths).expect("discover");

        let bare = &config.user_subagents[0];
        assert_eq!(bare.name, "bare");
        assert_eq!(bare.summary, None);
        assert_eq!(bare.model, None);
        assert!(bare.tools.is_empty());
        assert_eq!(bare.max_turns, None);
    }

    #[test]
    fn nested_folders_are_scanned_and_join_the_id() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        let review = tree.user_agents_dir().join("review");
        tree.make_subagent(
            "security",
            &review,
            Some(&TempTree::subagent_markdown("security", "Security review.")),
        );
        tree.make_subagent("zeta", &tree.user_agents_dir(), None);

        let config = discover(&tree.paths).expect("discover");

        let ids: Vec<&str> = config
            .user_subagents
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(ids, vec!["user::review/security", "user::zeta"]);
    }

    #[test]
    fn only_markdown_files_count_and_dotfiles_are_skipped() {
        let tree = TempTree::new();
        tree.write_claude_json(json!({ "projects": {} }));
        let dir = tree.user_agents_dir();
        tree.make_subagent("kept", &dir, None);
        tree.write("not an agent", &dir.join("notes.txt"));
        tree.write("hidden", &dir.join(".hidden.md"));
        tree.write("hidden", &dir.join(".git").join("agent.md"));
        std::fs::write(
            dir.join("weird.md"),
            "---\nname: weird\ndescription: Weird.\nmaxTurns: lots\n---\n",
        )
        .expect("write");

        let config = discover(&tree.paths).expect("discover");

        let names: Vec<&str> = config
            .user_subagents
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, vec!["kept", "weird"]);
        assert_eq!(config.user_subagents[1].max_turns, None);
    }

    #[test]
    fn project_subagents_carry_the_project_scope() {
        let tree = TempTree::new();
        let project = tree.make_project("example-project");
        tree.write_claude_json(json!({
            "projects": { project.display().to_string(): {} }
        }));
        tree.make_subagent(
            "deployer",
            &tree.project_agents_dir(&project),
            Some(&TempTree::subagent_markdown("deployer", "Deploys.")),
        );

        let config = discover(&tree.paths).expect("discover");

        assert_eq!(config.project_subagents.len(), 1);
        let deployer = &config.project_subagents[0];
        assert_eq!(
            deployer.scope,
            Scope::Project {
                path: project.display().to_string()
            }
        );
        assert_eq!(deployer.id, format!("{}::deployer", deployer.scope.key()));
        assert_eq!(
            deployer.path,
            tree.project_agents_dir(&project).join("deployer.md")
        );
    }

    #[test]
    fn nested_project_scopes_never_share_an_id() {
        let tree = TempTree::new();
        let outer = tree.make_project("repo");
        let inner = tree.make_directory(&outer.join("packages").join("app"));
        tree.write_claude_json(json!({
            "projects": {
                outer.display().to_string(): {},
                inner.display().to_string(): {},
            }
        }));
        tree.make_subagent(
            "x",
            &tree.project_agents_dir(&outer).join("packages").join("app"),
            None,
        );
        tree.make_subagent("x", &tree.project_agents_dir(&inner), None);

        let config = discover(&tree.paths).expect("discover");

        assert_eq!(config.project_subagents.len(), 2);
        assert_ne!(
            config.project_subagents[0].id,
            config.project_subagents[1].id
        );
    }
}
