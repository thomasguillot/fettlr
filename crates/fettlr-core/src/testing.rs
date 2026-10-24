use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use crate::cli::{CliOutput, CliRunner};
use crate::cli_records::CoreError;
use crate::mcp::MonotonicClock;
use crate::paths::Paths;
use crate::skills::Clock;
use crate::trashing::Trashing;
use crate::updates::{GitProbing, PackageRegistry, RegistryClient};

#[derive(Clone, Debug)]
pub struct RecordedCall {
    pub binary: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
}

struct FakeCliRunnerState {
    calls: Vec<RecordedCall>,
    output: CliOutput,
    error: Option<CoreError>,
}

pub struct FakeCliRunner {
    state: Mutex<FakeCliRunnerState>,
}

impl FakeCliRunner {
    pub fn new() -> Self {
        Self::with_output(CliOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        })
    }

    pub fn with_stdout(stdout: impl Into<String>) -> Self {
        Self::with_output(CliOutput {
            stdout: stdout.into(),
            stderr: String::new(),
            exit_code: 0,
        })
    }

    pub fn with_output(output: CliOutput) -> Self {
        Self {
            state: Mutex::new(FakeCliRunnerState {
                calls: Vec::new(),
                output,
                error: None,
            }),
        }
    }

    pub fn with_error(error: CoreError) -> Self {
        let runner = Self::new();
        runner.state.lock().expect("lock poisoned").error = Some(error);
        runner
    }

    pub fn calls(&self) -> Vec<RecordedCall> {
        self.state.lock().expect("lock poisoned").calls.clone()
    }

    pub fn last_call(&self) -> Option<RecordedCall> {
        self.state
            .lock()
            .expect("lock poisoned")
            .calls
            .last()
            .cloned()
    }

    pub fn call_count(&self) -> usize {
        self.state.lock().expect("lock poisoned").calls.len()
    }
}

impl Default for FakeCliRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl CliRunner for FakeCliRunner {
    fn run(
        &self,
        binary: &Path,
        args: &[String],
        cwd: Option<&Path>,
        timeout: Duration,
    ) -> Result<CliOutput, CoreError> {
        let mut state = self.state.lock().expect("lock poisoned");
        state.calls.push(RecordedCall {
            binary: binary.to_path_buf(),
            args: args.to_vec(),
            cwd: cwd.map(Path::to_path_buf),
            timeout,
        });
        if let Some(error) = state.error.take() {
            return Err(error);
        }
        Ok(state.output.clone())
    }
}

pub struct FakeTrasher {
    trashed: Mutex<Vec<PathBuf>>,
}

impl FakeTrasher {
    pub fn new() -> Self {
        Self {
            trashed: Mutex::new(Vec::new()),
        }
    }

    pub fn trashed(&self) -> Vec<PathBuf> {
        self.trashed.lock().expect("lock poisoned").clone()
    }
}

impl Default for FakeTrasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Trashing for FakeTrasher {
    fn trash(&self, path: &Path) -> Result<(), CoreError> {
        if !path.exists() {
            return Err(CoreError::Trash {
                path: path.display().to_string(),
                reason: "path does not exist".to_string(),
            });
        }
        self.trashed
            .lock()
            .expect("lock poisoned")
            .push(path.to_path_buf());
        Ok(())
    }
}

pub struct FakeClock {
    now: Mutex<String>,
}

impl FakeClock {
    pub fn new(now: impl Into<String>) -> Self {
        Self {
            now: Mutex::new(now.into()),
        }
    }
}

impl Clock for FakeClock {
    fn now_rfc3339(&self) -> String {
        self.now.lock().expect("lock poisoned").clone()
    }
}

pub struct FakeMonotonicClock {
    now: Mutex<Instant>,
}

impl FakeMonotonicClock {
    pub fn new(now: Instant) -> Self {
        Self {
            now: Mutex::new(now),
        }
    }
}

impl MonotonicClock for FakeMonotonicClock {
    fn now(&self) -> Instant {
        *self.now.lock().expect("lock poisoned")
    }
}

pub struct FakeGitProbe {
    counts: HashMap<String, u32>,
    failures: HashSet<String>,
    panics: HashSet<String>,
    sources: HashMap<String, (String, BTreeMap<String, String>)>,
    unreachable: HashSet<String>,
    fetches: Mutex<HashMap<String, usize>>,
    exported: Mutex<Vec<(String, PathBuf)>>,
}

impl FakeGitProbe {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
            failures: HashSet::new(),
            panics: HashSet::new(),
            sources: HashMap::new(),
            unreachable: HashSet::new(),
            fetches: Mutex::new(HashMap::new()),
            exported: Mutex::new(Vec::new()),
        }
    }

    pub fn with_counts<K: Into<String>>(counts: impl IntoIterator<Item = (K, u32)>) -> Self {
        Self {
            counts: counts
                .into_iter()
                .map(|(path, count)| (path.into(), count))
                .collect(),
            ..Self::new()
        }
    }

    pub fn with_counts_and_failures(
        counts: impl IntoIterator<Item = (&'static str, u32)>,
        failures: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        Self {
            counts: counts
                .into_iter()
                .map(|(path, count)| (path.to_string(), count))
                .collect(),
            failures: failures.into_iter().map(str::to_string).collect(),
            ..Self::new()
        }
    }

    pub fn with_counts_and_panics(
        counts: impl IntoIterator<Item = (&'static str, u32)>,
        panics: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        Self {
            counts: counts
                .into_iter()
                .map(|(path, count)| (path.to_string(), count))
                .collect(),
            panics: panics.into_iter().map(str::to_string).collect(),
            ..Self::new()
        }
    }

    pub fn with_sources(
        sources: impl IntoIterator<
            Item = (
                &'static str,
                &'static str,
                BTreeMap<&'static str, &'static str>,
            ),
        >,
    ) -> Self {
        Self {
            sources: sources
                .into_iter()
                .map(|(url, commit, folders)| {
                    (
                        url.to_string(),
                        (
                            commit.to_string(),
                            folders
                                .into_iter()
                                .map(|(folder, sha)| (folder.to_string(), sha.to_string()))
                                .collect(),
                        ),
                    )
                })
                .collect(),
            ..Self::new()
        }
    }

    pub fn with_unreachable_source(mut self, url: &'static str) -> Self {
        self.unreachable.insert(url.to_string());
        self
    }

    pub fn fetch_count(&self, url: &str) -> usize {
        self.fetches
            .lock()
            .expect("lock poisoned")
            .get(url)
            .copied()
            .unwrap_or(0)
    }

    pub fn exports(&self) -> Vec<(String, PathBuf)> {
        self.exported.lock().expect("lock poisoned").clone()
    }

    fn unreachable_error() -> CoreError {
        CoreError::Cli {
            command: "git ls-remote".into(),
            stderr: "unreachable".into(),
        }
    }
}

impl Default for FakeGitProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl GitProbing for FakeGitProbe {
    fn behind_count(&self, repo: &Path) -> Result<u32, CoreError> {
        let key = repo.display().to_string();
        if self.panics.contains(&key) {
            panic!("FakeGitProbe configured to panic for {key}");
        }
        if self.failures.contains(&key) {
            return Err(CoreError::NoUpstream { repo: key });
        }
        Ok(self.counts.get(&key).copied().unwrap_or(0))
    }

    fn pull_ff_only(&self, _repo: &Path) -> Result<(), CoreError> {
        Ok(())
    }

    fn fetch_source(
        &self,
        url: &str,
        _reference: Option<&str>,
        _cache_repo: &Path,
    ) -> Result<String, CoreError> {
        *self
            .fetches
            .lock()
            .expect("lock poisoned")
            .entry(url.to_string())
            .or_insert(0) += 1;
        if self.unreachable.contains(url) {
            return Err(Self::unreachable_error());
        }
        self.sources
            .get(url)
            .map(|(commit, _)| commit.clone())
            .ok_or_else(Self::unreachable_error)
    }

    fn tree_sha(
        &self,
        _cache_repo: &Path,
        commit: &str,
        folder: &str,
    ) -> Result<Option<String>, CoreError> {
        Ok(self
            .sources
            .values()
            .find(|(source_commit, _)| source_commit == commit)
            .and_then(|(_, folders)| folders.get(folder).cloned()))
    }

    fn export_folder(
        &self,
        _cache_repo: &Path,
        commit: &str,
        folder: &str,
        destination: &Path,
    ) -> Result<(), CoreError> {
        let target = destination.join(folder);
        std::fs::create_dir_all(&target)?;
        std::fs::write(target.join("SKILL.md"), format!("exported {commit}"))?;
        self.exported
            .lock()
            .expect("lock poisoned")
            .push((folder.to_string(), destination.to_path_buf()));
        Ok(())
    }
}

pub struct FakeRegistryClient {
    versions: HashMap<String, String>,
    hanging: HashSet<String>,
}

impl FakeRegistryClient {
    pub fn new() -> Self {
        Self {
            versions: HashMap::new(),
            hanging: HashSet::new(),
        }
    }

    pub fn with_versions(versions: impl IntoIterator<Item = (&'static str, &'static str)>) -> Self {
        Self {
            versions: versions
                .into_iter()
                .map(|(package, version)| (package.to_string(), version.to_string()))
                .collect(),
            hanging: HashSet::new(),
        }
    }

    pub fn with_versions_and_hanging(
        versions: impl IntoIterator<Item = (&'static str, &'static str)>,
        hanging: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        Self {
            versions: versions
                .into_iter()
                .map(|(package, version)| (package.to_string(), version.to_string()))
                .collect(),
            hanging: hanging.into_iter().map(str::to_string).collect(),
        }
    }

    fn key(registry: PackageRegistry, package: &str) -> String {
        let prefix = match registry {
            PackageRegistry::Npm => "npm",
            PackageRegistry::PyPi => "pypi",
        };
        format!("{prefix}:{package}")
    }
}

impl Default for FakeRegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistryClient for FakeRegistryClient {
    fn latest_version(
        &self,
        registry: PackageRegistry,
        package: &str,
    ) -> Result<String, CoreError> {
        let key = Self::key(registry, package);
        if self.hanging.contains(&key) {
            std::thread::sleep(Duration::from_secs(2));
        }
        self.versions
            .get(&key)
            .cloned()
            .ok_or_else(|| CoreError::RegistryRequestFailed {
                package: package.to_string(),
                reason: "no fake version configured".to_string(),
            })
    }
}

/// A throwaway `~/.claude` layout under a `tempfile::TempDir`. Every test builds its own and the
/// tree deletes itself when it is dropped, so nothing ever points at a real home.
pub struct TempTree {
    dir: TempDir,
    pub paths: Paths,
}

impl TempTree {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("create temp dir");
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("create claude dir");
        let paths = Paths {
            claude_json: dir.path().join(".claude.json"),
            support_dir: dir.path().join("Support"),
            binary_override: None,
            claude_dir,
        };
        Self { dir, paths }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn user_skills_dir(&self) -> PathBuf {
        self.paths.skills_dir()
    }

    pub fn project_skills_dir(&self, project: &Path) -> PathBuf {
        project.join(".claude").join("skills")
    }

    pub fn user_agents_dir(&self) -> PathBuf {
        self.paths.agents_dir()
    }

    pub fn project_agents_dir(&self, project: &Path) -> PathBuf {
        project.join(".claude").join("agents")
    }

    pub fn make_subagent(&self, name: &str, directory: &Path, markdown: Option<&str>) -> PathBuf {
        let file = directory.join(format!("{name}.md"));
        let body = markdown
            .map(str::to_string)
            .unwrap_or_else(|| format!("# {name}\n"));
        self.write(&body, &file)
    }

    pub fn subagent_markdown(name: &str, description: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: {description}\nmodel: sonnet\ntools: Read, Grep, Bash\neffort: medium\npermissionMode: default\nmaxTurns: 12\nmemory: project\n---\n\n# {name}\n\nBody text the parser must ignore.\n"
        )
    }

    pub fn make_directory(&self, path: &Path) -> PathBuf {
        std::fs::create_dir_all(path).expect("create directory");
        path.to_path_buf()
    }

    pub fn write(&self, contents: &str, path: &Path) -> PathBuf {
        if let Some(parent) = path.parent() {
            self.make_directory(parent);
        }
        std::fs::write(path, contents).expect("write file");
        path.to_path_buf()
    }

    pub fn write_skills_lock(&self, skills: serde_json::Value) {
        let lock = serde_json::json!({
            "version": 3,
            "skills": skills,
            "dismissed": {"findSkillsPrompt": true},
            "lastSelectedAgents": ["claude-code"]
        });
        self.write(
            &serde_json::to_string_pretty(&lock).expect("serialize lock"),
            &self.paths.skills_cli_lock(),
        );
    }

    pub fn make_skills_cli_skill(&self, name: &str) -> PathBuf {
        let canonical = self.paths.skills_cli_skills_dir().join(name);
        self.write(&format!("# {name}\n"), &canonical.join("SKILL.md"));
        let link = self.paths.skills_dir().join(name);
        self.make_directory(&self.paths.skills_dir());
        std::os::unix::fs::symlink(format!("../../.agents/skills/{name}"), &link)
            .expect("link skill");
        link
    }

    pub fn write_claude_json_raw(&self, raw: &str) {
        self.write(raw, &self.paths.claude_json);
    }

    pub fn write_claude_json(&self, value: serde_json::Value) {
        let raw = serde_json::to_string_pretty(&value).expect("serialize json");
        self.write_claude_json_raw(&raw);
    }

    /// A project directory that exists on disk.
    pub fn make_project(&self, name: &str) -> PathBuf {
        let path = self.root().join("Sites").join(name);
        self.make_directory(&path)
    }

    /// A project path that is recorded in `.claude.json` but absent from disk.
    pub fn absent_project(&self, name: &str) -> PathBuf {
        self.root().join("Sites").join(name)
    }

    pub fn make_skill(
        &self,
        name: &str,
        directory: &Path,
        markdown: Option<&str>,
        git_clone: bool,
    ) -> PathBuf {
        let folder = self.make_directory(&directory.join(name));
        if let Some(markdown) = markdown {
            self.write(markdown, &folder.join("SKILL.md"));
        }
        if git_clone {
            self.make_directory(&folder.join(".git"));
        }
        folder
    }

    /// Where Claude Code unpacks a plugin, and the skills folder inside it.
    pub fn plugin_install_dir(&self, name: &str) -> PathBuf {
        self.paths
            .claude_dir
            .join("plugins")
            .join("cache")
            .join(name)
    }

    pub fn plugin_skills_dir(&self, name: &str) -> PathBuf {
        self.plugin_install_dir(name).join("skills")
    }

    pub fn plugin_agents_dir(&self, name: &str) -> PathBuf {
        self.plugin_install_dir(name).join("agents")
    }

    pub fn write_marketplace(
        &self,
        name: &str,
        source: serde_json::Value,
        plugins: serde_json::Value,
    ) -> PathBuf {
        let dir = self.paths.marketplaces_dir().join(name);
        let manifest = serde_json::json!({
            "name": name,
            "owner": {"name": "Test"},
            "plugins": plugins,
        });
        self.write(
            &serde_json::to_string_pretty(&manifest).expect("serialize json"),
            &dir.join(".claude-plugin").join("marketplace.json"),
        );
        let known_path = self.paths.known_marketplaces_json();
        let mut known: serde_json::Value = std::fs::read_to_string(&known_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        known[name] = serde_json::json!({
            "source": source,
            "installLocation": dir,
            "lastUpdated": "2026-10-24T00:00:00.000Z",
        });
        self.write(
            &serde_json::to_string_pretty(&known).expect("serialize json"),
            &known_path,
        );
        dir
    }

    pub fn write_plugin_manifest(
        &self,
        marketplace_dir: &Path,
        relative: &str,
        version: Option<&str>,
    ) -> PathBuf {
        let plugin_dir = marketplace_dir.join(relative);
        let name = Path::new(relative)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut manifest = serde_json::json!({ "name": name });
        if let Some(version) = version {
            manifest["version"] = serde_json::json!(version);
        }
        self.write(
            &serde_json::to_string_pretty(&manifest).expect("serialize json"),
            &plugin_dir.join(".claude-plugin").join("plugin.json"),
        )
    }

    pub fn make_skill_symlink(&self, name: &str, directory: &Path, target: &Path) -> PathBuf {
        self.make_directory(directory);
        let link = directory.join(name);
        std::os::unix::fs::symlink(target, &link).expect("create symlink");
        link
    }

    pub fn skill_markdown(name: &str, description: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nBody text that the parser must ignore.\n"
        )
    }
}

impl Default for TempTree {
    fn default() -> Self {
        Self::new()
    }
}
