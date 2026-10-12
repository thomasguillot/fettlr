mod claude_json;
mod store;

use serde::{Deserialize, Serialize};

use crate::cli::ClaudeCli;
use crate::cli_records::{CoreError, McpServerConfig};
use crate::models::Scope;

pub use claude_json::set_project_mcpjson_server;
pub use store::{McpStore, MonotonicClock};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpOrigin {
    UserScope,
    LocalScope { project: String },
    ProjectFile { project: String },
    Plugin { plugin_id: String },
}

impl McpOrigin {
    pub fn key(&self) -> String {
        match self {
            McpOrigin::UserScope => "user".to_string(),
            McpOrigin::LocalScope { project } => format!("local:{project}"),
            McpOrigin::ProjectFile { project } => format!("project:{project}"),
            McpOrigin::Plugin { plugin_id } => format!("plugin:{plugin_id}"),
        }
    }

    /// The CLI scope a mutation runs at, and whether it is the `local` variant of a project scope.
    pub(crate) fn mutable_scope(&self) -> Result<(Scope, bool), CoreError> {
        match self {
            McpOrigin::UserScope => Ok((Scope::User, false)),
            McpOrigin::LocalScope { project } => Ok((
                Scope::Project {
                    path: project.clone(),
                },
                true,
            )),
            McpOrigin::ProjectFile { project } => Ok((
                Scope::Project {
                    path: project.clone(),
                },
                false,
            )),
            McpOrigin::Plugin { plugin_id } => Err(CoreError::ManagedByPlugin {
                plugin_id: plugin_id.clone(),
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub origin: McpOrigin,
    pub config: McpServerConfig,
    pub is_enabled: bool,
    pub transport: String,
}

impl McpServer {
    pub fn new(name: &str, origin: McpOrigin, config: McpServerConfig, is_enabled: bool) -> Self {
        Self {
            id: format!("{}/{name}", origin.key()),
            name: name.to_string(),
            transport: transport_of(&config),
            origin,
            config,
            is_enabled,
        }
    }
}

fn transport_of(config: &McpServerConfig) -> String {
    match config.kind.as_deref() {
        Some(kind) if !kind.is_empty() => kind.to_string(),
        _ if config.url.is_none() => "stdio".to_string(),
        _ => "http".to_string(),
    }
}

pub trait McpMutating: Send + Sync {
    fn mcp_add_json(
        &self,
        name: &str,
        config_json: &str,
        scope: &Scope,
        local: bool,
    ) -> Result<(), CoreError>;

    fn mcp_remove(&self, name: &str, scope: &Scope, local: bool) -> Result<(), CoreError>;
}

impl McpMutating for ClaudeCli {
    fn mcp_add_json(
        &self,
        name: &str,
        config_json: &str,
        scope: &Scope,
        local: bool,
    ) -> Result<(), CoreError> {
        ClaudeCli::mcp_add_json(self, name, config_json, scope, local)
    }

    fn mcp_remove(&self, name: &str, scope: &Scope, local: bool) -> Result<(), CoreError> {
        ClaudeCli::mcp_remove(self, name, scope, local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(json: serde_json::Value) -> McpServerConfig {
        serde_json::from_value(json).expect("decode config")
    }

    #[test]
    fn origin_keys_name_their_scope() {
        assert_eq!(McpOrigin::UserScope.key(), "user");
        assert_eq!(
            McpOrigin::LocalScope {
                project: "/a".to_string()
            }
            .key(),
            "local:/a"
        );
        assert_eq!(
            McpOrigin::ProjectFile {
                project: "/a".to_string()
            }
            .key(),
            "project:/a"
        );
        assert_eq!(
            McpOrigin::Plugin {
                plugin_id: "x@y".to_string()
            }
            .key(),
            "plugin:x@y"
        );
    }

    #[test]
    fn an_id_joins_the_origin_key_and_the_name() {
        let server = McpServer::new(
            "context7",
            McpOrigin::UserScope,
            config(serde_json::json!({"command": "npx"})),
            true,
        );
        assert_eq!(server.id, "user/context7");
    }

    #[test]
    fn transport_falls_back_to_the_shape_of_the_config() {
        assert_eq!(transport_of(&config(serde_json::json!({}))), "stdio");
        assert_eq!(
            transport_of(&config(serde_json::json!({"command": "npx"}))),
            "stdio"
        );
        assert_eq!(
            transport_of(&config(serde_json::json!({"url": "https://example.com"}))),
            "http"
        );
        assert_eq!(
            transport_of(&config(
                serde_json::json!({"type": "sse", "url": "https://example.com"})
            )),
            "sse"
        );
        assert_eq!(
            transport_of(&config(
                serde_json::json!({"type": "", "url": "https://example.com"})
            )),
            "http"
        );
    }
}
