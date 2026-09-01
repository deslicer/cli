//! Scaffold generator for `.deslicer/environments/<env>.yml`.
//!
//! Byte-compatible header and destination shape with Observer
//! `github_repo_sync/yaml.rs` / the DAI portal generator.

/// Customer-repo directory for Deslicer environment files.
pub const DESLICER_ENVIRONMENTS_DIR: &str = ".deslicer/environments";

/// Repo-relative path for an environment file.
pub fn environment_config_file_path(environment_name: &str) -> String {
    format!("{DESLICER_ENVIRONMENTS_DIR}/{environment_name}.yml")
}

pub fn build_header(environment_name: &str, tenant_label: &str) -> Vec<String> {
    vec![
        "# Deslicer environment configuration.".to_string(),
        format!(
            "# File stem \"{environment_name}\" maps to a workspace environment (tenant: {tenant_label})."
        ),
        "# Add apps under each inventory_group as `- source_path: <relative-app-path>`."
            .to_string(),
        "# See README.md at the repository root for how this file is used.".to_string(),
        String::new(),
    ]
}

pub fn placeholder_destinations() -> Vec<String> {
    [
        "# TODO: No machine groups found for this workspace yet. Apply",
        "# Enterprise Inventory, then re-save the mapping in the Deslicer",
        "# dashboard to populate destinations.",
        "destinations: []",
    ]
    .map(String::from)
    .to_vec()
}

pub fn dedupe_group_names(host_group_names: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::new();
    for group in host_group_names {
        let name = group.trim();
        if name.is_empty() || !seen.insert(name.to_string()) {
            continue;
        }
        names.push(name.to_string());
    }
    names
}

/// Inventory group names are plain YAML scalars in the common case.
/// Quote defensively only when a name would break unquoted YAML.
pub fn format_scalar(value: &str) -> String {
    if is_plain_scalar(value) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "''"))
    }
}

fn is_plain_scalar(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-'))
}

#[cfg(test)]
mod tests {
    use super::format_scalar;

    #[test]
    fn quotes_non_plain_scalars() {
        assert_eq!(format_scalar("group with spaces"), "'group with spaces'");
        assert_eq!(format_scalar("AIO_5"), "AIO_5");
    }
}
