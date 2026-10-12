use fettlr_core::{PackageRegistry, PackageSpec};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn scoped_npx_package_is_unpinned() {
    let spec = PackageSpec::parse(Some("npx"), Some(&args(&["-y", "@upstash/context7-mcp"])))
        .expect("parses");
    assert_eq!(spec.registry, PackageRegistry::Npm);
    assert_eq!(spec.name, "@upstash/context7-mcp");
    assert_eq!(spec.pinned_version, None);
}

#[test]
fn latest_tag_counts_as_unpinned() {
    let spec = PackageSpec::parse(Some("npx"), Some(&args(&["-y", "@accesslint/mcp@latest"])))
        .expect("parses");
    assert_eq!(spec.name, "@accesslint/mcp");
    assert_eq!(spec.pinned_version, None);
}

#[test]
fn explicit_npm_version_pins() {
    let spec = PackageSpec::parse(Some("npx"), Some(&args(&["pkg@2.1.0"]))).expect("parses");
    assert_eq!(spec.registry, PackageRegistry::Npm);
    assert_eq!(spec.name, "pkg");
    assert_eq!(spec.pinned_version.as_deref(), Some("2.1.0"));
}

#[test]
fn scoped_npm_version_splits_on_last_at() {
    let spec = PackageSpec::parse(Some("npx"), Some(&args(&["--yes", "@scope/pkg@1.2.3"])))
        .expect("parses");
    assert_eq!(spec.name, "@scope/pkg");
    assert_eq!(spec.pinned_version.as_deref(), Some("1.2.3"));
}

#[test]
fn uvx_double_equals_pins() {
    let spec =
        PackageSpec::parse(Some("uvx"), Some(&args(&["mcp-server-fetch==1.0"]))).expect("parses");
    assert_eq!(spec.registry, PackageRegistry::PyPi);
    assert_eq!(spec.name, "mcp-server-fetch");
    assert_eq!(spec.pinned_version.as_deref(), Some("1.0"));
}

#[test]
fn uvx_at_version_pins() {
    let spec = PackageSpec::parse(Some("uvx"), Some(&args(&["pkg@1.0"]))).expect("parses");
    assert_eq!(spec.registry, PackageRegistry::PyPi);
    assert_eq!(spec.name, "pkg");
    assert_eq!(spec.pinned_version.as_deref(), Some("1.0"));
}

#[test]
fn uvx_bare_package_is_unpinned() {
    let spec = PackageSpec::parse(Some("uvx"), Some(&args(&["mcp-server-fetch"]))).expect("parses");
    assert_eq!(spec.registry, PackageRegistry::PyPi);
    assert_eq!(spec.name, "mcp-server-fetch");
    assert_eq!(spec.pinned_version, None);
}

#[test]
fn package_flag_value_is_the_spec() {
    let spec = PackageSpec::parse(
        Some("npx"),
        Some(&args(&["-p", "some-package", "some-binary"])),
    )
    .expect("parses");
    assert_eq!(spec.name, "some-package");
    assert_eq!(spec.pinned_version, None);
}

#[test]
fn bunx_uses_npm() {
    let spec = PackageSpec::parse(Some("bunx"), Some(&args(&["pkg@3.0.0"]))).expect("parses");
    assert_eq!(spec.registry, PackageRegistry::Npm);
    assert_eq!(spec.pinned_version.as_deref(), Some("3.0.0"));
}

#[test]
fn runner_path_resolves_to_its_name() {
    let spec = PackageSpec::parse(
        Some("/opt/homebrew/bin/npx"),
        Some(&args(&["-y", "pkg@1.0.0"])),
    )
    .expect("parses");
    assert_eq!(spec.registry, PackageRegistry::Npm);
    assert_eq!(spec.name, "pkg");
}

#[test]
fn non_runner_command_has_no_spec() {
    assert_eq!(
        PackageSpec::parse(Some("node"), Some(&args(&["server.js"]))),
        None
    );
}

#[test]
fn missing_command_or_arguments_has_no_spec() {
    assert_eq!(PackageSpec::parse(None, Some(&args(&["pkg"]))), None);
    assert_eq!(PackageSpec::parse(Some("npx"), None), None);
    assert_eq!(PackageSpec::parse(Some("npx"), Some(&args(&[]))), None);
    assert_eq!(PackageSpec::parse(Some("npx"), Some(&args(&["-y"]))), None);
}

#[test]
fn dist_tag_is_unpinned() {
    let spec = PackageSpec::parse(Some("npx"), Some(&args(&["pkg@next"]))).expect("parses");
    assert_eq!(spec.name, "pkg");
    assert_eq!(spec.pinned_version, None);
}

#[test]
fn version_range_is_unpinned() {
    assert_eq!(
        PackageSpec::parse(Some("npx"), Some(&args(&["pkg@^1.2.0"])))
            .and_then(|spec| spec.pinned_version),
        None
    );
    assert_eq!(
        PackageSpec::parse(Some("npx"), Some(&args(&["pkg@~1.0"])))
            .and_then(|spec| spec.pinned_version),
        None
    );
    assert_eq!(
        PackageSpec::parse(Some("uvx"), Some(&args(&["pkg@beta"])))
            .and_then(|spec| spec.pinned_version),
        None
    );
}

#[test]
fn trailing_at_without_version_is_unpinned() {
    let spec = PackageSpec::parse(Some("npx"), Some(&args(&["pkg@"]))).expect("parses");
    assert_eq!(spec.name, "pkg");
    assert_eq!(spec.pinned_version, None);
}

#[test]
fn command_matching_is_case_insensitive() {
    let spec = PackageSpec::parse(Some("NPX"), Some(&args(&["pkg@1.0.0"]))).expect("parses");
    assert_eq!(spec.registry, PackageRegistry::Npm);
    assert_eq!(spec.name, "pkg");
    assert_eq!(spec.pinned_version.as_deref(), Some("1.0.0"));

    let spec = PackageSpec::parse(Some("Uvx"), Some(&args(&["pkg@1.0"]))).expect("parses");
    assert_eq!(spec.registry, PackageRegistry::PyPi);

    let spec = PackageSpec::parse(Some("/opt/homebrew/bin/BunX"), Some(&args(&["pkg@2.0.0"])))
        .expect("parses");
    assert_eq!(spec.registry, PackageRegistry::Npm);
}

#[test]
fn only_exact_versions_pin() {
    let pinned = |runner: &str, token: &str| {
        PackageSpec::parse(Some(runner), Some(&args(&[token]))).and_then(|spec| spec.pinned_version)
    };
    assert_eq!(pinned("npx", "pkg@1"), None);
    assert_eq!(pinned("npx", "pkg@1.2"), None);
    assert_eq!(pinned("npx", "pkg@1.2.x"), None);
    assert_eq!(pinned("npx", "pkg@1.2.*"), None);
    assert_eq!(pinned("uvx", "pkg==1.2.*"), None);
    assert_eq!(pinned("uvx", "pkg==1"), None);
    assert_eq!(pinned("npx", "pkg@1.2.3").as_deref(), Some("1.2.3"));
    assert_eq!(
        pinned("npx", "pkg@2.0.0-rc.1").as_deref(),
        Some("2.0.0-rc.1")
    );
    assert_eq!(
        pinned("npx", "pkg@1.2.3+build.5").as_deref(),
        Some("1.2.3+build.5")
    );
    assert_eq!(pinned("uvx", "pkg==1.2").as_deref(), Some("1.2"));
}
