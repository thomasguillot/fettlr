use std::path::Path;

use super::registry::PackageRegistry;

#[derive(Clone, Debug, PartialEq)]
pub struct PackageSpec {
    pub registry: PackageRegistry,
    pub name: String,
    pub pinned_version: Option<String>,
}

impl PackageSpec {
    pub fn parse(command: Option<&str>, args: Option<&[String]>) -> Option<PackageSpec> {
        let command = command?;
        let args = args?;

        let registry = match runner_name(command).to_lowercase().as_str() {
            "npx" | "bunx" => PackageRegistry::Npm,
            "uvx" => PackageRegistry::PyPi,
            _ => return None,
        };

        let index = token_index(args)?;
        Some(spec(registry, &args[index]))
    }
}

fn runner_name(command: &str) -> &str {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
}

fn token_index(args: &[String]) -> Option<usize> {
    args.iter()
        .position(|arg| !arg.is_empty() && !arg.starts_with('-'))
}

fn spec(registry: PackageRegistry, token: &str) -> PackageSpec {
    if registry == PackageRegistry::PyPi
        && let Some(index) = token.find("==")
    {
        let name = token[..index].to_string();
        let pinned_version = pinned_version(registry, &token[index + 2..]);
        return PackageSpec {
            registry,
            name,
            pinned_version,
        };
    }

    match token.rfind('@') {
        Some(index) if index != 0 => {
            let name = token[..index].to_string();
            let pinned_version = pinned_version(registry, &token[index + 1..]);
            PackageSpec {
                registry,
                name,
                pinned_version,
            }
        }
        _ => PackageSpec {
            registry,
            name: token.to_string(),
            pinned_version: None,
        },
    }
}

/// Only an exact version pins. A dist-tag (`next`), a range (`^1.2.0`, `1`, `1.2.*`) or a bare
/// major floats, so treating it as a pin would compare it against a version number and badge a
/// permanent update.
fn pinned_version(registry: PackageRegistry, raw: &str) -> Option<String> {
    let minimum_fields = if registry == PackageRegistry::Npm {
        3
    } else {
        2
    };
    is_exact_version(raw, minimum_fields).then(|| raw.to_string())
}

fn is_exact_version(raw: &str, minimum_fields: usize) -> bool {
    let core = raw.split(['-', '+']).next().unwrap_or("");
    let rest = &raw[core.len()..];
    let fields: Vec<&str> = core.split('.').collect();
    let numeric = fields
        .iter()
        .all(|field| !field.is_empty() && field.chars().all(|c| c.is_ascii_digit()));
    let suffix_ok = rest
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    numeric && fields.len() >= minimum_fields && suffix_ok
}
