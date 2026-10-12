use std::time::Duration;

use serde::Deserialize;
use ureq::Agent;

use crate::cli_records::CoreError;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PackageRegistry {
    Npm,
    PyPi,
}

pub trait RegistryClient: Send + Sync {
    fn latest_version(&self, registry: PackageRegistry, package: &str)
    -> Result<String, CoreError>;
}

#[derive(Debug, Default)]
pub struct HttpRegistryClient;

impl HttpRegistryClient {
    const TIMEOUT: Duration = Duration::from_secs(10);

    fn agent() -> Agent {
        Agent::config_builder()
            .timeout_global(Some(Self::TIMEOUT))
            .build()
            .into()
    }
}

impl RegistryClient for HttpRegistryClient {
    fn latest_version(
        &self,
        registry: PackageRegistry,
        package: &str,
    ) -> Result<String, CoreError> {
        if !is_usable(package, registry) {
            return Err(CoreError::InvalidPackageName {
                package: package.to_string(),
            });
        }

        let url = match registry {
            PackageRegistry::Npm => format!("https://registry.npmjs.org/{package}"),
            PackageRegistry::PyPi => format!("https://pypi.org/pypi/{package}/json"),
        };

        let mut response = Self::agent()
            .get(&url)
            .call()
            .map_err(|error| lookup_failed(package, &error))?;

        let version = match registry {
            PackageRegistry::Npm => response
                .body_mut()
                .read_json::<NpmPackage>()
                .map_err(|error| lookup_failed(package, &error))?
                .dist_tags
                .and_then(|tags| tags.latest),
            PackageRegistry::PyPi => response
                .body_mut()
                .read_json::<PyPiPackage>()
                .map_err(|error| lookup_failed(package, &error))?
                .info
                .and_then(|info| info.version),
        };

        version
            .filter(|version| !version.is_empty())
            .ok_or_else(|| CoreError::RegistryMalformedResponse {
                package: package.to_string(),
            })
    }
}

fn lookup_failed(package: &str, error: &ureq::Error) -> CoreError {
    CoreError::RegistryRequestFailed {
        package: package.to_string(),
        reason: error.to_string(),
    }
}

/// A package name reaches us from config files, so it is refused unless it can only ever be one
/// path component (two for an npm scope), never a traversal into another registry endpoint.
fn is_usable(package: &str, registry: PackageRegistry) -> bool {
    if package.is_empty() || package.chars().count() > 214 || package.contains("..") {
        return false;
    }
    if package
        .chars()
        .any(|c| c.is_whitespace() || !is_url_path_char(c))
    {
        return false;
    }
    if registry == PackageRegistry::Npm
        && let Some(rest) = package.strip_prefix('@')
    {
        let mut parts = rest.splitn(2, '/');
        let scope = parts.next().unwrap_or_default();
        return match parts.next() {
            Some(name) => {
                !scope.is_empty()
                    && !name.is_empty()
                    && !name.contains('/')
                    && !scope.contains('@')
                    && !name.contains('@')
            }
            None => false,
        };
    }
    !package.contains('/') && !package.contains('@')
}

fn is_url_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-._~!$&'()*+,;=:@/".contains(c)
}

#[derive(Deserialize)]
struct NpmPackage {
    #[serde(rename = "dist-tags")]
    dist_tags: Option<DistTags>,
}

#[derive(Deserialize)]
struct DistTags {
    latest: Option<String>,
}

#[derive(Deserialize)]
struct PyPiPackage {
    info: Option<PyPiInfo>,
}

#[derive(Deserialize)]
struct PyPiInfo {
    version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_npm_names_are_usable() {
        assert!(is_usable("@upstash/context7-mcp", PackageRegistry::Npm));
    }

    #[test]
    fn bare_names_are_usable() {
        assert!(is_usable("mcp-server-fetch", PackageRegistry::PyPi));
    }

    #[test]
    fn traversal_attempts_are_rejected() {
        assert!(!is_usable("../etc/passwd", PackageRegistry::Npm));
        assert!(!is_usable("pkg/../other", PackageRegistry::PyPi));
    }

    #[test]
    fn slash_separated_names_are_rejected_outside_npm_scopes() {
        assert!(!is_usable("@scope/pkg", PackageRegistry::PyPi));
        assert!(!is_usable("scope/pkg", PackageRegistry::Npm));
    }

    #[test]
    fn whitespace_is_rejected() {
        assert!(!is_usable("pkg name", PackageRegistry::Npm));
    }

    #[test]
    fn empty_name_is_rejected() {
        assert!(!is_usable("", PackageRegistry::Npm));
    }

    #[test]
    fn an_npm_body_reports_the_latest_dist_tag() {
        let package: NpmPackage = serde_json::from_str(
            r#"{"name":"pkg","dist-tags":{"latest":"1.4.0","next":"2.0.0-beta"}}"#,
        )
        .expect("valid npm body");

        assert_eq!(
            package.dist_tags.and_then(|tags| tags.latest).as_deref(),
            Some("1.4.0")
        );
    }

    #[test]
    fn a_pypi_body_reports_the_version_under_info() {
        let package: PyPiPackage =
            serde_json::from_str(r#"{"info":{"name":"pkg","version":"0.9.1"},"releases":{}}"#)
                .expect("valid pypi body");

        assert_eq!(
            package.info.and_then(|info| info.version).as_deref(),
            Some("0.9.1")
        );
    }

    #[test]
    fn a_body_that_publishes_no_version_reports_nothing() {
        let npm: NpmPackage = serde_json::from_str(r#"{"name":"pkg"}"#).expect("valid npm body");
        assert!(npm.dist_tags.and_then(|tags| tags.latest).is_none());

        let pypi: PyPiPackage =
            serde_json::from_str(r#"{"info":{"name":"pkg"}}"#).expect("valid pypi body");
        assert!(pypi.info.and_then(|info| info.version).is_none());
    }
}
