use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    User,
    Project {
        path: String,
    },
    /// A plugin installed with `-s local`: private to the reader, inside one project.
    Local {
        path: String,
    },
    /// A plugin scope Fettlr lists but cannot act on, such as `synced` or `managed`, carrying the
    /// CLI's own word for it.
    Other {
        name: String,
    },
}

impl Scope {
    pub fn key(&self) -> String {
        match self {
            Scope::User => "user".to_string(),
            Scope::Project { path } => format!("project:{path}"),
            Scope::Local { path } => format!("local:{path}"),
            Scope::Other { name } => format!("other:{name}"),
        }
    }

    /// The value the CLI's `-s` flag takes for this scope, or `None` for a scope no Fettlr verb
    /// may pass to the CLI.
    pub fn cli_value(&self) -> Option<&'static str> {
        match self {
            Scope::User => Some("user"),
            Scope::Project { .. } => Some("project"),
            Scope::Local { .. } => Some("local"),
            Scope::Other { .. } => None,
        }
    }

    pub fn project_path(&self) -> Option<&str> {
        match self {
            Scope::User | Scope::Other { .. } => None,
            Scope::Project { path } | Scope::Local { path } => Some(path),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum UpdateState {
    Unknown,
    UpToDate,
    Checking,
    UpdateAvailable { installed: String, latest: String },
    Behind { commits: u32 },
    TracksLatest,
}

impl UpdateState {
    /// The one place that decides whether an item is due for an update. `update_all` works through
    /// exactly the items this reports true for, so anything deciding for itself would count work the
    /// run then skips and report success over an empty run.
    ///
    /// A catalogue entry with no version reads as `latest: "unknown"`, which says nothing about
    /// whether the installed copy is stale, so treating it as due would be an invented claim.
    pub fn is_stale(&self) -> bool {
        match self {
            UpdateState::Behind { commits } => *commits > 0,
            UpdateState::UpdateAvailable { latest, .. } => latest != UNKNOWN_VERSION,
            _ => false,
        }
    }
}

/// What a record with no version of its own carries, from `PluginRecord`'s decoder on down.
pub const UNKNOWN_VERSION: &str = "unknown";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_scope_key_and_cli_value() {
        assert_eq!(Scope::User.key(), "user");
        assert_eq!(Scope::User.cli_value(), Some("user"));
        assert_eq!(Scope::User.project_path(), None);
    }

    #[test]
    fn project_scope_key_and_cli_value() {
        let scope = Scope::Project {
            path: "/Users/dev/Sites/example-project".to_string(),
        };
        assert_eq!(scope.key(), "project:/Users/dev/Sites/example-project");
        assert_eq!(scope.cli_value(), Some("project"));
        assert_eq!(
            scope.project_path(),
            Some("/Users/dev/Sites/example-project")
        );
    }

    #[test]
    fn local_scope_key_and_cli_value() {
        let scope = Scope::Local {
            path: "/Users/dev/Sites/example-project".to_string(),
        };
        assert_eq!(scope.key(), "local:/Users/dev/Sites/example-project");
        assert_eq!(scope.cli_value(), Some("local"));
        assert_eq!(
            scope.project_path(),
            Some("/Users/dev/Sites/example-project")
        );
    }

    #[test]
    fn other_scope_has_no_cli_value_or_project_path() {
        let scope = Scope::Other {
            name: "synced".to_string(),
        };
        assert_eq!(scope.key(), "other:synced");
        assert_eq!(scope.cli_value(), None);
        assert_eq!(scope.project_path(), None);
    }

    #[test]
    fn distinct_projects_are_distinct_scopes() {
        assert_ne!(
            Scope::Project {
                path: "/a".to_string()
            },
            Scope::Project {
                path: "/b".to_string()
            }
        );
        assert_ne!(
            Scope::Project {
                path: "/a".to_string()
            },
            Scope::User
        );
    }

    #[test]
    fn scope_round_trips_through_serde_json() {
        for scope in [
            Scope::User,
            Scope::Project {
                path: "/Users/dev/Sites/example-project".to_string(),
            },
            Scope::Local {
                path: "/Users/dev/Sites/example-project".to_string(),
            },
            Scope::Other {
                name: "synced".to_string(),
            },
        ] {
            let json = serde_json::to_string(&scope).expect("serializes");
            let decoded: Scope = serde_json::from_str(&json).expect("round trips");
            assert_eq!(decoded, scope);
        }
    }

    #[test]
    fn behind_and_a_known_newer_version_are_the_only_stale_states() {
        assert!(UpdateState::Behind { commits: 1 }.is_stale());
        assert!(
            UpdateState::UpdateAvailable {
                installed: "1.0.0".to_string(),
                latest: "2.0.0".to_string()
            }
            .is_stale()
        );

        assert!(!UpdateState::Behind { commits: 0 }.is_stale());
        assert!(!UpdateState::Unknown.is_stale());
        assert!(!UpdateState::UpToDate.is_stale());
        assert!(!UpdateState::Checking.is_stale());
        assert!(!UpdateState::TracksLatest.is_stale());
    }

    /// The state `update_all` refuses to act on, so nothing may badge it as pending either.
    #[test]
    fn a_catalogue_entry_with_no_version_is_never_stale() {
        assert!(
            !UpdateState::UpdateAvailable {
                installed: "1.0.0".to_string(),
                latest: UNKNOWN_VERSION.to_string()
            }
            .is_stale()
        );
    }

    #[test]
    fn update_state_compares_by_payload() {
        assert_eq!(
            UpdateState::UpdateAvailable {
                installed: "1.0.0".to_string(),
                latest: "1.1.0".to_string()
            },
            UpdateState::UpdateAvailable {
                installed: "1.0.0".to_string(),
                latest: "1.1.0".to_string()
            }
        );
        assert_ne!(
            UpdateState::UpdateAvailable {
                installed: "1.0.0".to_string(),
                latest: "1.1.0".to_string()
            },
            UpdateState::UpdateAvailable {
                installed: "1.0.0".to_string(),
                latest: "2.0.0".to_string()
            }
        );
        assert_ne!(
            UpdateState::Behind { commits: 3 },
            UpdateState::Behind { commits: 4 }
        );
        assert_ne!(UpdateState::Unknown, UpdateState::UpToDate);
        assert_eq!(UpdateState::TracksLatest, UpdateState::TracksLatest);
    }
}
