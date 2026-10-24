use fettlr_core::testing::TempTree;
use fettlr_core::{CatalogueVersion, read_plugin_catalogue};
use serde_json::json;

const SHA: &str = "9847f2aa1a15f25df35ed1fb7b4557dbb60cd651";

fn github() -> serde_json::Value {
    json!({"source": "github", "repo": "acme/market"})
}

fn version(v: &str) -> Option<CatalogueVersion> {
    Some(CatalogueVersion::Version(v.into()))
}

#[test]
fn the_plugin_manifest_version_beats_the_entry_version() {
    let tree = TempTree::new();
    let dir = tree.write_marketplace(
        "accesslint",
        github(),
        json!([{"name": "accesslint", "version": "0.10.2", "source": "./plugins/accesslint"}]),
    );
    tree.write_plugin_manifest(&dir, "./plugins/accesslint", Some("0.10.3"));
    let catalogue = read_plugin_catalogue(&tree.paths);
    assert_eq!(
        catalogue.get("accesslint@accesslint").cloned(),
        version("0.10.3")
    );
}

#[test]
fn the_entry_version_is_used_when_the_manifest_has_none() {
    let tree = TempTree::new();
    tree.write_marketplace(
        "m",
        github(),
        json!([{"name": "p", "version": "1.0.0", "source": "./p"}]),
    );
    assert_eq!(
        read_plugin_catalogue(&tree.paths).get("p@m").cloned(),
        version("1.0.0")
    );
}

#[test]
fn a_manifest_without_a_version_falls_back_to_the_entry() {
    let tree = TempTree::new();
    let dir = tree.write_marketplace(
        "m",
        github(),
        json!([{"name": "p", "version": "2.15.0", "source": "./p"}]),
    );
    tree.write_plugin_manifest(&dir, "./p", None);
    assert_eq!(
        read_plugin_catalogue(&tree.paths).get("p@m").cloned(),
        version("2.15.0")
    );
}

#[test]
fn a_relative_plugin_with_no_version_anywhere_is_untracked() {
    let tree = TempTree::new();
    let dir = tree.write_marketplace("m", github(), json!([{"name": "p", "source": "./p"}]));
    tree.write_plugin_manifest(&dir, "./p", None);
    assert_eq!(
        read_plugin_catalogue(&tree.paths).get("p@m"),
        Some(&CatalogueVersion::Untracked)
    );
}

#[test]
fn a_relative_plugin_in_a_directory_marketplace_is_untracked() {
    let tree = TempTree::new();
    let dir = tree.write_marketplace(
        "m",
        json!({"source": "directory", "path": "/somewhere"}),
        json!([{"name": "p", "version": "1.0.0", "source": "./p"}]),
    );
    tree.write_plugin_manifest(&dir, "./p", Some("1.1.0"));
    assert_eq!(
        read_plugin_catalogue(&tree.paths).get("p@m"),
        Some(&CatalogueVersion::Untracked)
    );
}

#[test]
fn a_relative_source_that_leaves_the_marketplace_is_untracked() {
    let tree = TempTree::new();
    let dir = tree.write_marketplace(
        "m",
        github(),
        json!([
            {"name": "up", "source": "../outside"},
            {"name": "abs", "version": "1.0.0", "source": "/abs/path"}
        ]),
    );
    let outside = dir.parent().unwrap().join("outside");
    tree.write(
        r#"{"name": "outside", "version": "9.9.9"}"#,
        &outside.join(".claude-plugin").join("plugin.json"),
    );
    let catalogue = read_plugin_catalogue(&tree.paths);
    assert_eq!(catalogue.get("up@m"), Some(&CatalogueVersion::Untracked));
    assert_eq!(catalogue.get("abs@m"), Some(&CatalogueVersion::Untracked));
}

#[test]
fn a_remote_entry_version_is_used() {
    let tree = TempTree::new();
    tree.write_marketplace(
        "m",
        github(),
        json!([{"name": "design", "version": "1.2.0", "source": {
            "source": "git-subdir", "url": "https://x/y", "path": "design", "sha": SHA
        }}]),
    );
    assert_eq!(
        read_plugin_catalogue(&tree.paths).get("design@m").cloned(),
        version("1.2.0")
    );
}

#[test]
fn a_versionless_url_entry_tracks_its_pinned_commit() {
    let tree = TempTree::new();
    tree.write_marketplace(
        "m",
        github(),
        json!([
            {"name": "u", "source": {"source": "url", "url": "https://x/y.git", "sha": SHA}},
            {"name": "g", "source": {"source": "github", "repo": "a/b", "sha": SHA}}
        ]),
    );
    let catalogue = read_plugin_catalogue(&tree.paths);
    assert_eq!(
        catalogue.get("u@m"),
        Some(&CatalogueVersion::Commit(SHA.into()))
    );
    assert_eq!(
        catalogue.get("g@m"),
        Some(&CatalogueVersion::Commit(SHA.into()))
    );
}

#[test]
fn a_versionless_git_subdir_or_ref_only_entry_is_untracked() {
    let tree = TempTree::new();
    tree.write_marketplace(
        "m",
        github(),
        json!([
            {"name": "s", "source": {"source": "git-subdir", "url": "https://x/y", "path": "s", "sha": SHA}},
            {"name": "r", "source": {"source": "url", "url": "https://x/y.git", "ref": "main"}}
        ]),
    );
    let catalogue = read_plugin_catalogue(&tree.paths);
    assert_eq!(catalogue.get("s@m"), Some(&CatalogueVersion::Untracked));
    assert_eq!(catalogue.get("r@m"), Some(&CatalogueVersion::Untracked));
}

#[test]
fn npm_and_archive_entries_are_untracked() {
    let tree = TempTree::new();
    tree.write_marketplace(
        "m",
        github(),
        json!([
            {"name": "n", "version": "1.0.0", "source": {"source": "npm", "package": "n"}},
            {"name": "a", "version": "1.0.0", "source": {"source": "archive", "url": "https://x/a.zip"}}
        ]),
    );
    let catalogue = read_plugin_catalogue(&tree.paths);
    assert_eq!(catalogue.get("n@m"), Some(&CatalogueVersion::Untracked));
    assert_eq!(catalogue.get("a@m"), Some(&CatalogueVersion::Untracked));
}

#[test]
fn an_unreadable_manifest_drops_only_its_own_marketplace() {
    let tree = TempTree::new();
    let broken = tree.write_marketplace(
        "broken",
        github(),
        json!([{"name": "p", "version": "1.0.0", "source": "./p"}]),
    );
    tree.write_marketplace(
        "fine",
        github(),
        json!([{"name": "q", "version": "2.0.0", "source": "./q"}]),
    );
    tree.write(
        "{not json",
        &broken.join(".claude-plugin").join("marketplace.json"),
    );
    let catalogue = read_plugin_catalogue(&tree.paths);
    assert_eq!(catalogue.get("p@broken"), None);
    assert_eq!(catalogue.get("q@fine").cloned(), version("2.0.0"));
}

#[test]
fn the_first_entry_wins_when_a_name_repeats() {
    let tree = TempTree::new();
    tree.write_marketplace(
        "m",
        github(),
        json!([
            {"name": "p", "version": "1.0.0", "source": "./a"},
            {"name": "p", "version": "2.0.0", "source": "./b"}
        ]),
    );
    assert_eq!(
        read_plugin_catalogue(&tree.paths).get("p@m").cloned(),
        version("1.0.0")
    );
}

#[test]
fn no_known_marketplaces_file_is_an_empty_catalogue() {
    let tree = TempTree::new();
    let catalogue = read_plugin_catalogue(&tree.paths);
    assert!(catalogue.is_empty());
    assert_eq!(catalogue.len(), 0);
}

#[test]
fn a_malformed_entry_drops_only_itself() {
    let tree = TempTree::new();
    tree.write_marketplace(
        "m",
        github(),
        json!([
            {"name": "bad", "version": 3, "source": "./bad"},
            {"name": "good", "version": "1.0.0", "source": "./good"}
        ]),
    );
    let catalogue = read_plugin_catalogue(&tree.paths);
    assert_eq!(catalogue.get("good@m").cloned(), version("1.0.0"));
    assert_eq!(catalogue.get("bad@m"), None);
}

#[test]
fn a_malformed_known_marketplace_drops_only_itself() {
    let tree = TempTree::new();
    tree.write_marketplace(
        "bad",
        github(),
        json!([{"name": "p", "version": "1.0.0", "source": "./p"}]),
    );
    tree.write_marketplace(
        "fine",
        github(),
        json!([{"name": "q", "version": "2.0.0", "source": "./q"}]),
    );
    let known_path = tree.paths.known_marketplaces_json();
    let mut known: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&known_path).expect("read known"))
            .expect("parse known");
    known["bad"]["installLocation"] = json!(7);
    tree.write(&known.to_string(), &known_path);
    let catalogue = read_plugin_catalogue(&tree.paths);
    assert_eq!(catalogue.get("q@fine").cloned(), version("2.0.0"));
    assert_eq!(catalogue.get("p@bad"), None);
}

#[test]
fn a_git_marketplace_source_reads_relative_versions() {
    let tree = TempTree::new();
    let dir = tree.write_marketplace(
        "m",
        json!({"source": "git", "url": "git@example.com:acme/market.git"}),
        json!([{"name": "p", "source": "./p"}]),
    );
    tree.write_plugin_manifest(&dir, "./p", Some("3.2.1"));
    assert_eq!(
        read_plugin_catalogue(&tree.paths).get("p@m").cloned(),
        version("3.2.1")
    );
}

#[test]
fn the_install_location_is_honoured() {
    let tree = TempTree::new();
    let dir = tree.write_marketplace(
        "m",
        github(),
        json!([{"name": "p", "version": "1.0.0", "source": "./p"}]),
    );
    let elsewhere = tree.root().join("elsewhere").join("m");
    tree.make_directory(elsewhere.parent().expect("parent"));
    std::fs::rename(&dir, &elsewhere).expect("move marketplace");
    let known_path = tree.paths.known_marketplaces_json();
    let mut known: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&known_path).expect("read known"))
            .expect("parse known");
    known["m"]["installLocation"] = json!(elsewhere);
    tree.write(&known.to_string(), &known_path);
    assert!(!dir.exists());
    assert_eq!(
        read_plugin_catalogue(&tree.paths).get("p@m").cloned(),
        version("1.0.0")
    );
}

#[test]
fn a_missing_install_location_falls_back_to_the_marketplaces_dir() {
    let tree = TempTree::new();
    tree.write_marketplace(
        "m",
        github(),
        json!([{"name": "p", "version": "1.0.0", "source": "./p"}]),
    );
    let known_path = tree.paths.known_marketplaces_json();
    let mut known: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&known_path).expect("read known"))
            .expect("parse known");
    known["m"]
        .as_object_mut()
        .expect("record")
        .remove("installLocation");
    tree.write(&known.to_string(), &known_path);
    assert_eq!(
        read_plugin_catalogue(&tree.paths).get("p@m").cloned(),
        version("1.0.0")
    );
}

const GCS_SHA: &str = "58267944f92187b7d6a1c822165c96c81c864806";

#[test]
fn a_versionless_relative_plugin_tracks_the_downloaded_marketplace_commit() {
    let tree = TempTree::new();
    let dir = tree.write_marketplace(
        "m",
        github(),
        json!([
            {"name": "bare", "source": "./plugins/bare"},
            {"name": "versioned", "source": "./plugins/versioned"}
        ]),
    );
    tree.write(&format!("{GCS_SHA}\n"), &dir.join(".gcs-sha"));
    tree.write_plugin_manifest(&dir, "./plugins/bare", None);
    tree.write_plugin_manifest(&dir, "./plugins/versioned", Some("2.0.0"));
    let catalogue = read_plugin_catalogue(&tree.paths);
    assert_eq!(
        catalogue.get("bare@m"),
        Some(&CatalogueVersion::Commit(GCS_SHA.into()))
    );
    assert_eq!(catalogue.get("versioned@m").cloned(), version("2.0.0"));
}

#[test]
fn a_malformed_sha_file_or_a_real_clone_leaves_a_versionless_relative_plugin_untracked() {
    for (contents, clone) in [(&GCS_SHA[..39], false), (GCS_SHA, true)] {
        let tree = TempTree::new();
        let dir = tree.write_marketplace(
            "m",
            github(),
            json!([{"name": "bare", "source": "./plugins/bare"}]),
        );
        tree.write(contents, &dir.join(".gcs-sha"));
        if clone {
            tree.make_directory(&dir.join(".git"));
        }
        assert_eq!(
            read_plugin_catalogue(&tree.paths).get("bare@m"),
            Some(&CatalogueVersion::Untracked)
        );
    }
}
