use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fettlr_core::testing::FakeCliRunner;
use fettlr_core::{ClaudeCli, CliOutput, CliRunner, CoreError, Paths, ProcessCliRunner, Scope};

const PLUGIN_LIST: &str = include_str!("fixtures/plugin-list.json");
const PLUGIN_LIST_AVAILABLE: &str = include_str!("fixtures/plugin-list-available.json");
const MARKETPLACE_LIST: &str = include_str!("fixtures/marketplace-list.json");

const PROJECT: &str = "/fake/home/Sites/example-project";

fn write_executable(path: &Path) {
    std::fs::create_dir_all(path.parent().expect("has parent")).expect("create dirs");
    std::fs::write(path, "#!/bin/sh\necho fake-claude\n").expect("write file");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod");
}

fn fake_binary_path() -> PathBuf {
    let path = std::env::temp_dir().join("fettlr-core-tests-fake-claude");
    write_executable(&path);
    path
}

fn paths_with_override() -> Paths {
    Paths {
        claude_dir: PathBuf::from("/fake/home/.claude"),
        claude_json: PathBuf::from("/fake/home/.claude.json"),
        support_dir: PathBuf::from("/fake/support"),
        binary_override: Some(fake_binary_path()),
    }
}

fn cli_with(runner: FakeCliRunner, paths: Paths) -> (Arc<FakeCliRunner>, ClaudeCli) {
    let runner = Arc::new(runner);
    let cli = ClaudeCli::new(runner.clone(), &paths);
    (runner, cli)
}

fn cli(runner: FakeCliRunner) -> (Arc<FakeCliRunner>, ClaudeCli) {
    cli_with(runner, paths_with_override())
}

fn homebrew_or_usr_local_claude_present() -> bool {
    Path::new("/opt/homebrew/bin/claude").exists() || Path::new("/usr/local/bin/claude").exists()
}

mod reads {
    use super::*;

    #[test]
    fn list_plugins_sends_json_flag_and_parses_every_record() {
        let (runner, cli) = cli(FakeCliRunner::with_stdout(PLUGIN_LIST));
        let plugins = cli.list_plugins().expect("valid fixture");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "list".to_string(),
                "--json".to_string()
            ]
        );
        assert_eq!(call.cwd, None);
        assert_eq!(call.binary, fake_binary_path());
        assert_eq!(call.timeout, Duration::from_secs(60));
        assert_eq!(plugins.len(), 4);
        assert!(plugins.iter().all(|p| !p.is_available_only));
    }

    #[test]
    fn plugin_catalogue_sends_the_available_flag() {
        let (runner, cli) = cli(FakeCliRunner::with_stdout(PLUGIN_LIST_AVAILABLE));
        let catalogue = cli.plugin_catalogue().expect("valid fixture");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "list".to_string(),
                "--json".to_string(),
                "--available".to_string(),
            ]
        );
        assert_eq!(catalogue.len(), 5);
        assert!(catalogue.iter().all(|p| p.is_available_only));
        assert!(
            catalogue
                .iter()
                .any(|p| p.plugin_id == "linear@claude-plugins-official")
        );
    }

    #[test]
    fn list_marketplaces_parses_every_record() {
        let (runner, cli) = cli(FakeCliRunner::with_stdout(MARKETPLACE_LIST));
        let markets = cli.list_marketplaces().expect("valid fixture");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "marketplace".to_string(),
                "list".to_string(),
                "--json".to_string(),
            ]
        );
        assert_eq!(call.timeout, Duration::from_secs(60));
        assert_eq!(markets.len(), 3);
        assert_eq!(
            markets.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            vec!["accesslint", "claude-plugins-official", "internal-devkit"]
        );
    }

    #[test]
    fn plugin_details_returns_raw_stdout() {
        let (runner, cli) = cli(FakeCliRunner::with_stdout("Commands: 3\nSkills: 12\n"));
        let text = cli
            .plugin_details("impeccable@impeccable")
            .expect("stdout captured");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "details".to_string(),
                "impeccable@impeccable".to_string(),
            ]
        );
        assert_eq!(call.cwd, None);
        assert_eq!(text, "Commands: 3\nSkills: 12\n");
    }

    #[test]
    fn mcp_get_without_project_runs_without_working_directory() {
        let (runner, cli) = cli(FakeCliRunner::with_stdout(
            "context7: npx -y @upstash/context7-mcp",
        ));
        let text = cli.mcp_get("context7", None).expect("stdout captured");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec!["mcp".to_string(), "get".to_string(), "context7".to_string()]
        );
        assert_eq!(call.cwd, None);
        assert_eq!(text, "context7: npx -y @upstash/context7-mcp");
    }

    #[test]
    fn mcp_get_with_project_runs_in_that_directory() {
        let (runner, cli) = cli(FakeCliRunner::with_stdout(""));
        let _ = cli
            .mcp_get("linear", Some(PROJECT))
            .expect("stdout captured");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec!["mcp".to_string(), "get".to_string(), "linear".to_string()]
        );
        assert_eq!(call.cwd, Some(PathBuf::from(PROJECT)));
    }
}

mod mutations {
    use super::*;

    #[test]
    fn install_plugin_at_user_scope() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.install_plugin("x@m", &Scope::User).expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "install".to_string(),
                "x@m".to_string(),
                "-s".to_string(),
                "user".to_string(),
                "-y".to_string(),
            ]
        );
        assert_eq!(call.cwd, None);
        assert_eq!(call.timeout, Duration::from_secs(60));
    }

    #[test]
    fn install_plugin_at_project_scope_runs_in_the_project_directory() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.install_plugin(
            "x@m",
            &Scope::Project {
                path: PROJECT.to_string(),
            },
        )
        .expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "install".to_string(),
                "x@m".to_string(),
                "-s".to_string(),
                "project".to_string(),
                "-y".to_string(),
            ]
        );
        assert_eq!(call.cwd, Some(PathBuf::from(PROJECT)));
    }

    #[test]
    fn uninstall_plugin_omits_the_yes_flag() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.uninstall_plugin("x@m", &Scope::User).expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "uninstall".to_string(),
                "x@m".to_string(),
                "-s".to_string(),
                "user".to_string(),
            ]
        );
        assert!(!call.args.contains(&"--prune".to_string()));
    }

    #[test]
    fn enable_plugin() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.set_plugin_enabled(
            "x@m",
            true,
            &Scope::Project {
                path: PROJECT.to_string(),
            },
        )
        .expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "enable".to_string(),
                "x@m".to_string(),
                "-s".to_string(),
                "project".to_string(),
            ]
        );
        assert_eq!(call.cwd, Some(PathBuf::from(PROJECT)));
    }

    #[test]
    fn disable_plugin() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.set_plugin_enabled("x@m", false, &Scope::User)
            .expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "disable".to_string(),
                "x@m".to_string(),
                "-s".to_string(),
                "user".to_string(),
            ]
        );
        assert_eq!(call.cwd, None);
    }

    #[test]
    fn update_plugin() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.update_plugin("x@m", &Scope::User).expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "update".to_string(),
                "x@m".to_string(),
                "-s".to_string(),
                "user".to_string(),
                "-y".to_string(),
            ]
        );
    }

    #[test]
    fn uninstall_plugin_at_local_scope_runs_in_the_project_directory() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.uninstall_plugin(
            "x@m",
            &Scope::Local {
                path: PROJECT.to_string(),
            },
        )
        .expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "uninstall".to_string(),
                "x@m".to_string(),
                "-s".to_string(),
                "local".to_string(),
            ]
        );
        assert_eq!(call.cwd, Some(PathBuf::from(PROJECT)));
    }

    #[test]
    fn every_plugin_verb_refuses_a_scope_fettlr_cannot_act_on() {
        let (runner, cli) = cli(FakeCliRunner::new());
        let synced = Scope::Other {
            name: "synced".to_string(),
        };
        let outcomes = [
            cli.install_plugin("x@m", &synced),
            cli.uninstall_plugin("x@m", &synced),
            cli.set_plugin_enabled("x@m", true, &synced),
            cli.set_plugin_enabled("x@m", false, &synced),
            cli.update_plugin("x@m", &synced),
        ];

        for outcome in outcomes {
            let err = outcome.expect_err("refused");
            assert!(matches!(&err, CoreError::UnsupportedScope { scope } if scope == "synced"));
            assert_eq!(
                err.to_string(),
                "Fettlr cannot change a plugin installed with the synced scope"
            );
        }
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn add_marketplace_passes_the_source_verbatim() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.add_marketplace("owner/repo").expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "marketplace".to_string(),
                "add".to_string(),
                "owner/repo".to_string(),
            ]
        );
        assert_eq!(call.cwd, None);
    }

    #[test]
    fn remove_marketplace() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.remove_marketplace("accesslint").expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "marketplace".to_string(),
                "remove".to_string(),
                "accesslint".to_string(),
            ]
        );
    }

    #[test]
    fn update_one_marketplace_uses_the_long_timeout() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.update_marketplace(Some("accesslint"))
            .expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "marketplace".to_string(),
                "update".to_string(),
                "accesslint".to_string(),
            ]
        );
        assert_eq!(call.timeout, Duration::from_secs(300));
    }

    #[test]
    fn update_all_marketplaces_omits_the_name() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.update_marketplace(None).expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "plugin".to_string(),
                "marketplace".to_string(),
                "update".to_string(),
            ]
        );
        assert_eq!(call.timeout, Duration::from_secs(300));
    }

    #[test]
    fn mcp_add_json_at_user_scope() {
        let (runner, cli) = cli(FakeCliRunner::new());
        let json = r#"{"command":"npx","args":["-y","pkg"]}"#;
        cli.mcp_add_json("pkg", json, &Scope::User, false)
            .expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "mcp".to_string(),
                "add-json".to_string(),
                "pkg".to_string(),
                json.to_string(),
                "-s".to_string(),
                "user".to_string(),
            ]
        );
        assert_eq!(call.cwd, None);
    }

    #[test]
    fn mcp_add_json_at_local_scope_uses_local_and_runs_in_the_project_directory() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.mcp_add_json(
            "pkg",
            "{}",
            &Scope::Project {
                path: PROJECT.to_string(),
            },
            true,
        )
        .expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "mcp".to_string(),
                "add-json".to_string(),
                "pkg".to_string(),
                "{}".to_string(),
                "-s".to_string(),
                "local".to_string(),
            ]
        );
        assert_eq!(call.cwd, Some(PathBuf::from(PROJECT)));
    }

    #[test]
    fn mcp_add_json_at_project_scope_runs_in_the_project_directory() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.mcp_add_json(
            "pkg",
            "{}",
            &Scope::Project {
                path: PROJECT.to_string(),
            },
            false,
        )
        .expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "mcp".to_string(),
                "add-json".to_string(),
                "pkg".to_string(),
                "{}".to_string(),
                "-s".to_string(),
                "project".to_string(),
            ]
        );
        assert_eq!(call.cwd, Some(PathBuf::from(PROJECT)));
    }

    #[test]
    fn mcp_remove_local_scope() {
        let (runner, cli) = cli(FakeCliRunner::new());
        cli.mcp_remove(
            "pkg",
            &Scope::Project {
                path: PROJECT.to_string(),
            },
            true,
        )
        .expect("succeeds");

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(
            call.args,
            vec![
                "mcp".to_string(),
                "remove".to_string(),
                "pkg".to_string(),
                "-s".to_string(),
                "local".to_string(),
            ]
        );
        assert_eq!(call.cwd, Some(PathBuf::from(PROJECT)));
    }
}

mod errors {
    use super::*;

    #[test]
    fn non_zero_exit_carries_stderr_and_the_command_line() {
        let runner = FakeCliRunner::with_output(CliOutput {
            stdout: String::new(),
            stderr: "Plugin not found in any marketplace".to_string(),
            exit_code: 1,
        });
        let (_runner, cli) = cli(runner);

        let err = cli
            .install_plugin("nope@nowhere", &Scope::User)
            .expect_err("non-zero exit is an error");
        match err {
            CoreError::Cli { command, stderr } => {
                assert_eq!(stderr, "Plugin not found in any marketplace");
                assert_eq!(command, "claude plugin install nope@nowhere -s user -y");
            }
            other => panic!("expected Cli error, got {other:?}"),
        }
    }

    #[test]
    fn malformed_plugin_json_is_bad_json() {
        let (_runner, cli) = cli(FakeCliRunner::with_stdout("not json at all"));
        let err = cli.list_plugins().expect_err("malformed json");
        assert!(matches!(err, CoreError::BadJson(_)));
    }

    #[test]
    fn malformed_marketplace_json_is_bad_json() {
        let (_runner, cli) = cli(FakeCliRunner::with_stdout(r#"[{"nope": true}]"#));
        let err = cli.list_marketplaces().expect_err("malformed json");
        assert!(matches!(err, CoreError::BadJson(_)));
    }

    #[test]
    fn runner_errors_are_relabelled_with_the_claude_command() {
        let runner = FakeCliRunner::with_error(CoreError::Timeout {
            command: "claude".to_string(),
        });
        let (_runner, cli) = cli(runner);

        let err = cli
            .update_marketplace(Some("accesslint"))
            .expect_err("runner timed out");
        match err {
            CoreError::Timeout { command } => {
                assert_eq!(command, "claude plugin marketplace update accesslint");
            }
            other => panic!("expected Timeout error, got {other:?}"),
        }
    }

    #[test]
    fn mcp_add_json_error_command_redacts_the_config_json() {
        let runner = FakeCliRunner::with_output(CliOutput {
            stdout: String::new(),
            stderr: "refused".to_string(),
            exit_code: 2,
        });
        let (_runner, cli) = cli(runner);

        let secret = r#"{"command":"npx","env":{"API_KEY":"sk-live-secret"}}"#;
        let err = cli
            .mcp_add_json("pkg", secret, &Scope::User, false)
            .expect_err("non-zero exit is an error");
        match err {
            CoreError::Cli { command, stderr } => {
                assert_eq!(command, "claude mcp add-json pkg <config redacted> -s user");
                assert!(!command.contains(secret));
                assert!(!command.contains("sk-live-secret"));
                assert_eq!(stderr, "refused");
            }
            other => panic!("expected Cli error, got {other:?}"),
        }
    }

    #[test]
    fn no_binary_anywhere_is_binary_not_found() {
        if homebrew_or_usr_local_claude_present() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("create claude dir");
        let paths = Paths {
            claude_dir,
            claude_json: dir.path().join(".claude.json"),
            support_dir: dir.path().join("support"),
            binary_override: None,
        };

        let (runner, cli) = cli_with(FakeCliRunner::new(), paths);
        let err = cli.list_plugins().expect_err("no binary anywhere");
        assert!(matches!(err, CoreError::BinaryNotFound));
        assert_eq!(runner.call_count(), 1);
    }

    #[test]
    fn every_variant_has_a_non_empty_display_message() {
        let bad_json = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let variants = vec![
            CoreError::BadJson(bad_json),
            CoreError::BinaryNotFound,
            CoreError::Cli {
                command: "claude plugin list --json".to_string(),
                stderr: "boom".to_string(),
            },
            CoreError::Timeout {
                command: "claude plugin list --json".to_string(),
            },
        ];
        for variant in variants {
            assert!(!variant.to_string().is_empty());
        }
    }
}

mod discovery {
    use super::*;

    #[test]
    fn override_wins_over_every_other_candidate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let claude_dir = dir.path().join(".claude");
        let override_path = dir.path().join("override-claude");
        write_executable(&override_path);
        write_executable(&claude_dir.join("local/claude"));

        let paths = Paths {
            claude_dir,
            claude_json: dir.path().join(".claude.json"),
            support_dir: dir.path().join("support"),
            binary_override: Some(override_path.clone()),
        };
        let (runner, cli) = cli_with(FakeCliRunner::new(), paths);
        assert_eq!(cli.binary().expect("found"), override_path);
        assert_eq!(runner.call_count(), 0);
    }

    #[test]
    fn claude_local_comes_before_dot_local_bin() {
        let dir = tempfile::tempdir().expect("tempdir");
        let claude_dir = dir.path().join(".claude");
        let local_claude = claude_dir.join("local/claude");
        let dot_local = dir.path().join(".local/bin/claude");
        write_executable(&local_claude);
        write_executable(&dot_local);

        let paths = Paths {
            claude_dir,
            claude_json: dir.path().join(".claude.json"),
            support_dir: dir.path().join("support"),
            binary_override: None,
        };
        let (_runner, cli) = cli_with(FakeCliRunner::new(), paths);
        assert_eq!(cli.binary().expect("found"), local_claude);
    }

    #[test]
    fn dot_local_hangs_off_the_parent_of_claude_dir() {
        if homebrew_or_usr_local_claude_present() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("create claude dir");
        let dot_local = dir.path().join(".local/bin/claude");
        write_executable(&dot_local);

        let paths = Paths {
            claude_dir,
            claude_json: dir.path().join(".claude.json"),
            support_dir: dir.path().join("support"),
            binary_override: None,
        };
        let (_runner, cli) = cli_with(FakeCliRunner::new(), paths);
        assert_eq!(cli.binary().expect("found"), dot_local);
    }

    #[test]
    fn login_shell_probe_is_the_last_resort() {
        if homebrew_or_usr_local_claude_present() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("create claude dir");
        let paths = Paths {
            claude_dir,
            claude_json: dir.path().join(".claude.json"),
            support_dir: dir.path().join("support"),
            binary_override: None,
        };

        let (runner, cli) = cli_with(
            FakeCliRunner::with_stdout("/somewhere/else/claude\n"),
            paths,
        );
        assert_eq!(
            cli.binary().expect("found"),
            PathBuf::from("/somewhere/else/claude")
        );

        let call = runner.last_call().expect("a call was recorded");
        assert_eq!(call.binary, PathBuf::from("/bin/zsh"));
        assert_eq!(
            call.args,
            vec!["-lc".to_string(), "command -v claude".to_string()]
        );
        assert_eq!(call.cwd, None);
    }

    #[test]
    fn unusable_override_falls_through_to_the_next_candidate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let claude_dir = dir.path().join(".claude");
        let local_claude = claude_dir.join("local/claude");
        write_executable(&local_claude);
        let missing_override = dir.path().join("gone/claude");

        let paths = Paths {
            claude_dir,
            claude_json: dir.path().join(".claude.json"),
            support_dir: dir.path().join("support"),
            binary_override: Some(missing_override),
        };
        let (_runner, cli) = cli_with(FakeCliRunner::new(), paths);
        assert_eq!(cli.binary().expect("found"), local_claude);
    }

    #[test]
    fn discovery_is_cached_across_calls() {
        if homebrew_or_usr_local_claude_present() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("create claude dir");
        let paths = Paths {
            claude_dir,
            claude_json: dir.path().join(".claude.json"),
            support_dir: dir.path().join("support"),
            binary_override: None,
        };

        let (runner, cli) = cli_with(FakeCliRunner::with_stdout("/somewhere/claude\n"), paths);

        let first = cli.binary().expect("found");
        let second = cli.binary().expect("found");
        assert_eq!(first, second);
        assert_eq!(runner.call_count(), 1);

        let _ = cli.list_plugins();
        assert_eq!(runner.call_count(), 2);
    }
}

mod process_cli_runner {
    use super::*;

    #[test]
    fn captures_stdout_and_zero_exit() {
        let runner = ProcessCliRunner;
        let output = runner
            .run(
                Path::new("/bin/echo"),
                &["hello".to_string()],
                None,
                Duration::from_secs(10),
            )
            .expect("runs");
        assert_eq!(output.stdout, "hello\n");
        assert!(output.stderr.is_empty());
        assert_eq!(output.exit_code, 0);
    }

    #[test]
    fn captures_stderr_and_the_exit_code() {
        let runner = ProcessCliRunner;
        let output = runner
            .run(
                Path::new("/bin/sh"),
                &["-c".to_string(), "echo oops >&2; exit 3".to_string()],
                None,
                Duration::from_secs(10),
            )
            .expect("runs");
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, "oops\n");
        assert_eq!(output.exit_code, 3);
    }

    #[test]
    fn runs_in_the_given_working_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let runner = ProcessCliRunner;
        let output = runner
            .run(
                Path::new("/bin/pwd"),
                &[],
                Some(dir.path()),
                Duration::from_secs(10),
            )
            .expect("runs");
        let printed = output.stdout.trim();
        let expected_suffix = dir
            .path()
            .file_name()
            .expect("has a name")
            .to_str()
            .expect("valid utf8");
        assert!(printed.ends_with(expected_suffix));
    }

    #[test]
    fn missing_binary_returns_binary_not_found() {
        let runner = ProcessCliRunner;
        let err = runner
            .run(
                Path::new("/nope/not-a-binary"),
                &[],
                None,
                Duration::from_secs(10),
            )
            .expect_err("binary does not exist");
        assert!(matches!(err, CoreError::BinaryNotFound));
    }

    #[test]
    fn missing_working_directory_is_reported_without_blaming_the_binary() {
        let dir = tempfile::tempdir().expect("temp dir");
        let gone = dir.path().join("deleted-project");
        let err = ProcessCliRunner
            .run(
                Path::new("/bin/echo"),
                &[],
                Some(&gone),
                Duration::from_secs(10),
            )
            .expect_err("directory does not exist");
        match err {
            CoreError::MissingProjectDirectory { path } => {
                assert_eq!(path, gone.display().to_string());
            }
            other => panic!("expected MissingProjectDirectory, got {other:?}"),
        }
    }

    #[test]
    fn missing_working_directory_through_the_cli_keeps_the_binary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let override_path = dir.path().join("override-claude");
        write_executable(&override_path);
        let gone = dir.path().join("deleted-project");
        let paths = Paths {
            claude_dir: dir.path().join(".claude"),
            claude_json: dir.path().join(".claude.json"),
            support_dir: dir.path().join("support"),
            binary_override: Some(override_path.clone()),
        };
        let cli = ClaudeCli::new(Arc::new(ProcessCliRunner), &paths);
        let err = cli
            .mcp_get("x", Some(gone.to_str().expect("utf8")))
            .expect_err("directory does not exist");
        assert!(matches!(err, CoreError::MissingProjectDirectory { .. }));
        assert_eq!(cli.binary().expect("still cached"), override_path);
    }

    #[test]
    fn slow_command_is_killed_and_reported_as_timeout() {
        let runner = ProcessCliRunner;
        let started = Instant::now();
        let err = runner
            .run(
                Path::new("/bin/sleep"),
                &["30".to_string()],
                None,
                Duration::from_millis(200),
            )
            .expect_err("times out");
        assert!(matches!(err, CoreError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_descendant_holding_the_pipes_cannot_outlast_the_timeout() {
        let runner = ProcessCliRunner;
        let started = Instant::now();
        // The shell exits at once, but the backgrounded sleep inherits stdout and keeps the write
        // end open well past the deadline.
        let err = runner
            .run(
                Path::new("/bin/sh"),
                &["-c".to_string(), "sleep 30 & echo []".to_string()],
                None,
                Duration::from_millis(200),
            )
            .expect_err("does not wait on the descendant");
        assert!(matches!(err, CoreError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn timeout_kills_the_whole_process_group_including_descendants() {
        let marker = unique_marker();
        let runner = ProcessCliRunner;
        let started = Instant::now();
        let err = runner
            .run(
                Path::new("/bin/sh"),
                &[
                    "-c".to_string(),
                    format!("trap '' TERM; {marker} & {marker}"),
                ],
                None,
                Duration::from_millis(300),
            )
            .expect_err("times out");
        assert!(matches!(err, CoreError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(1));

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut survivors = -1;
        while Instant::now() < deadline {
            survivors = count_processes_matching(&marker);
            if survivors == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }
        assert_eq!(survivors, 0);
    }

    fn unique_marker() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .subsec_nanos();
        format!("sleep {}", 100_000 + (nanos % 900_000))
    }

    fn count_processes_matching(marker: &str) -> i32 {
        let script = format!("ps -Ao command | grep '{marker}' | grep -v grep | wc -l");
        let runner = ProcessCliRunner;
        let output = runner
            .run(
                Path::new("/bin/sh"),
                &["-c".to_string(), script],
                None,
                Duration::from_secs(10),
            )
            .expect("runs");
        output.stdout.trim().parse().unwrap_or(-1)
    }

    #[test]
    fn large_output_is_not_truncated() {
        let runner = ProcessCliRunner;
        let line = "x".repeat(40);
        let script = format!("for i in $(seq 1 20000); do echo {line}; done");
        let output = runner
            .run(
                Path::new("/bin/sh"),
                &["-c".to_string(), script],
                None,
                Duration::from_secs(30),
            )
            .expect("runs");
        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout.len(), 20000 * 41);
    }
}

#[test]
fn a_binary_that_stops_spawning_is_looked_up_again_on_the_next_call() {
    let stub = std::env::temp_dir().join("fettlr-core-tests-vanishing-claude");
    write_executable(&stub);
    let paths = Paths {
        claude_dir: PathBuf::from("/fake/home/.claude"),
        claude_json: PathBuf::from("/fake/home/.claude.json"),
        support_dir: PathBuf::from("/fake/support"),
        binary_override: Some(stub.clone()),
    };
    let (runner, cli) = cli_with(FakeCliRunner::with_error(CoreError::BinaryNotFound), paths);

    assert!(matches!(cli.list_plugins(), Err(CoreError::BinaryNotFound)));
    assert_eq!(runner.calls().len(), 1);
    assert_eq!(runner.calls()[0].binary, stub);

    std::fs::remove_file(&stub).expect("remove the stub");
    let _ = cli.list_plugins();

    assert!(
        runner
            .calls()
            .iter()
            .skip(1)
            .all(|call| call.binary != stub),
        "the removed binary was reused from the cache instead of being looked up again"
    );
}
