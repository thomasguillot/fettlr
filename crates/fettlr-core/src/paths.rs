use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Paths {
    pub claude_dir: PathBuf,
    pub claude_json: PathBuf,
    pub support_dir: PathBuf,
    pub binary_override: Option<PathBuf>,
}

impl Paths {
    pub fn skills_dir(&self) -> PathBuf {
        self.claude_dir.join("skills")
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.claude_dir.join("agents")
    }

    /// Parked items sit beside what they were parked from rather than under the app's support
    /// directory, so losing Fettlr leaves them somewhere the reader can find and move back by
    /// hand. Claude Code discovers by exact directory name and does not read either of these.
    pub fn skills_disabled_dir(&self) -> PathBuf {
        self.claude_dir.join("skills-disabled")
    }

    pub fn agents_disabled_dir(&self) -> PathBuf {
        self.claude_dir.join("agents-disabled")
    }

    /// A disabled server is removed from Claude Code's config and its only copy lives here, so
    /// this file has to outlive the app the way the parked folders do.
    pub fn mcp_disabled_file(&self) -> PathBuf {
        self.claude_dir.join("mcp-disabled.json")
    }

    pub fn plugins_dir(&self) -> PathBuf {
        self.claude_dir.join("plugins")
    }

    pub fn marketplaces_dir(&self) -> PathBuf {
        self.plugins_dir().join("marketplaces")
    }

    pub fn known_marketplaces_json(&self) -> PathBuf {
        self.plugins_dir().join("known_marketplaces.json")
    }

    pub fn settings_json(&self) -> PathBuf {
        self.claude_dir.join("settings.json")
    }

    /// The skills CLI's `~/.agents` folder, which sits beside `claude_dir`. Not to be confused
    /// with `agents_dir`, which holds Claude Code subagents.
    pub fn skills_cli_dir(&self) -> PathBuf {
        self.claude_dir
            .parent()
            .unwrap_or(&self.claude_dir)
            .join(".agents")
    }

    pub fn skills_cli_lock(&self) -> PathBuf {
        self.skills_cli_dir().join(".skill-lock.json")
    }

    pub fn skills_cli_skills_dir(&self) -> PathBuf {
        self.skills_cli_dir().join("skills")
    }

    pub fn skill_sources_cache(&self) -> PathBuf {
        self.support_dir.join("skill-sources")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_paths() -> Paths {
        Paths {
            claude_dir: PathBuf::from("/tmp/fake-home/.claude"),
            claude_json: PathBuf::from("/tmp/fake-home/.claude.json"),
            support_dir: PathBuf::from("/tmp/fake-support"),
            binary_override: None,
        }
    }

    #[test]
    fn skills_dir_hangs_off_claude_dir() {
        assert_eq!(
            fake_paths().skills_dir(),
            PathBuf::from("/tmp/fake-home/.claude/skills")
        );
    }

    #[test]
    fn agents_dir_hangs_off_claude_dir() {
        assert_eq!(
            fake_paths().agents_dir(),
            PathBuf::from("/tmp/fake-home/.claude/agents")
        );
    }

    #[test]
    fn plugins_dir_hangs_off_claude_dir() {
        assert_eq!(
            fake_paths().plugins_dir(),
            PathBuf::from("/tmp/fake-home/.claude/plugins")
        );
    }

    #[test]
    fn marketplaces_dir_hangs_off_claude_dir_plugins() {
        assert_eq!(
            fake_paths().marketplaces_dir(),
            PathBuf::from("/tmp/fake-home/.claude/plugins/marketplaces")
        );
    }

    #[test]
    fn known_marketplaces_json_hangs_off_plugins_dir() {
        assert_eq!(
            fake_paths().known_marketplaces_json(),
            PathBuf::from("/tmp/fake-home/.claude/plugins/known_marketplaces.json")
        );
    }

    #[test]
    fn settings_json_hangs_off_claude_dir() {
        assert_eq!(
            fake_paths().settings_json(),
            PathBuf::from("/tmp/fake-home/.claude/settings.json")
        );
    }

    #[test]
    fn skills_cli_dir_sits_beside_claude_dir() {
        assert_eq!(
            fake_paths().skills_cli_dir(),
            PathBuf::from("/tmp/fake-home/.agents")
        );
    }

    #[test]
    fn skills_cli_lock_hangs_off_skills_cli_dir() {
        assert_eq!(
            fake_paths().skills_cli_lock(),
            PathBuf::from("/tmp/fake-home/.agents/.skill-lock.json")
        );
    }

    #[test]
    fn skills_cli_skills_dir_hangs_off_skills_cli_dir() {
        assert_eq!(
            fake_paths().skills_cli_skills_dir(),
            PathBuf::from("/tmp/fake-home/.agents/skills")
        );
    }

    #[test]
    fn skill_sources_cache_hangs_off_support_dir() {
        assert_eq!(
            fake_paths().skill_sources_cache(),
            PathBuf::from("/tmp/fake-support/skill-sources")
        );
    }

    #[test]
    fn binary_override_is_carried() {
        let mut paths = fake_paths();
        paths.binary_override = Some(PathBuf::from("/opt/bin/claude"));
        assert_eq!(
            paths.binary_override,
            Some(PathBuf::from("/opt/bin/claude"))
        );
    }
}
