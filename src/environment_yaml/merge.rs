//! Merge Observer host groups into an existing environment file.
//!
//! Unlike Observer `github_repo_sync`, the CLI **keeps** destinations that
//! still list `source_path` apps when the group disappears from Observer.

use super::generate::{
    build_header, dedupe_group_names, environment_config_file_path, format_scalar,
    placeholder_destinations,
};
use super::parse::{extract_apps_blocks, ExistingAppsBlock};

/// A generated / merged environment YAML file plus sync actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedEnvironmentYaml {
    pub path: String,
    pub content: String,
    pub host_group_placeholder: bool,
    pub added: Vec<String>,
    pub removed_empty: Vec<String>,
    pub blocked: Vec<BlockedDestination>,
}

/// A destination Observer no longer has, but the file still lists apps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedDestination {
    pub inventory_group: String,
    pub source_paths: Vec<String>,
}

/// Generate the scaffold YAML: each host group becomes a
/// `destinations[].inventory_group` entry with an empty `apps:` list.
pub fn generate_environment_yaml(
    environment_name: &str,
    tenant_label: &str,
    host_group_names: &[String],
) -> MergedEnvironmentYaml {
    merge_environment_yaml(environment_name, tenant_label, host_group_names, None)
}

/// Merge Observer groups with an existing file.
///
/// * New groups are appended with an empty `apps:` list.
/// * Existing `apps:` blocks for surviving groups are preserved.
/// * Removed groups with no `source_path` apps are dropped.
/// * Removed groups that still list apps are kept and reported as blocked.
pub fn merge_environment_yaml(
    environment_name: &str,
    tenant_label: &str,
    host_group_names: &[String],
    existing_content: Option<&str>,
) -> MergedEnvironmentYaml {
    let path = environment_config_file_path(environment_name);
    let group_names = dedupe_group_names(host_group_names);
    let existing_apps = existing_content
        .map(extract_apps_blocks)
        .unwrap_or_default();
    let actions = classify_sync_actions(&group_names, &existing_apps);

    let mut lines = build_header(environment_name, tenant_label);
    if group_names.is_empty() && actions.blocked.is_empty() {
        lines.extend(placeholder_destinations());
        return MergedEnvironmentYaml {
            path,
            content: format!("{}\n", lines.join("\n")),
            host_group_placeholder: true,
            added: actions.added,
            removed_empty: actions.removed_empty,
            blocked: actions.blocked,
        };
    }

    lines.push("destinations:".to_string());
    emit_observer_groups(&mut lines, &group_names, &existing_apps);
    emit_blocked_groups(&mut lines, &actions.blocked, &existing_apps);

    MergedEnvironmentYaml {
        path,
        content: format!("{}\n", lines.join("\n")),
        host_group_placeholder: false,
        added: actions.added,
        removed_empty: actions.removed_empty,
        blocked: actions.blocked,
    }
}

struct SyncActions {
    added: Vec<String>,
    removed_empty: Vec<String>,
    blocked: Vec<BlockedDestination>,
}

fn classify_sync_actions(
    observer_groups: &[String],
    existing_apps: &[ExistingAppsBlock],
) -> SyncActions {
    let observer_set: std::collections::HashSet<&str> =
        observer_groups.iter().map(String::as_str).collect();
    let existing_set: std::collections::HashSet<&str> = existing_apps
        .iter()
        .map(|block| block.group_name.as_str())
        .collect();

    let added = observer_groups
        .iter()
        .filter(|name| !existing_set.contains(name.as_str()))
        .cloned()
        .collect();

    let mut removed_empty = Vec::new();
    let mut blocked = Vec::new();
    for block in existing_apps {
        if observer_set.contains(block.group_name.as_str()) {
            continue;
        }
        let source_paths = block.source_paths();
        if source_paths.is_empty() {
            removed_empty.push(block.group_name.clone());
        } else {
            blocked.push(BlockedDestination {
                inventory_group: block.group_name.clone(),
                source_paths,
            });
        }
    }

    SyncActions {
        added,
        removed_empty,
        blocked,
    }
}

fn emit_observer_groups(
    lines: &mut Vec<String>,
    group_names: &[String],
    existing_apps: &[ExistingAppsBlock],
) {
    for name in group_names {
        emit_destination(
            lines,
            name,
            existing_apps.iter().find(|b| &b.group_name == name),
        );
    }
}

fn emit_blocked_groups(
    lines: &mut Vec<String>,
    blocked: &[BlockedDestination],
    existing_apps: &[ExistingAppsBlock],
) {
    for dest in blocked {
        emit_destination(
            lines,
            &dest.inventory_group,
            existing_apps
                .iter()
                .find(|block| block.group_name == dest.inventory_group),
        );
    }
}

fn emit_destination(lines: &mut Vec<String>, name: &str, existing: Option<&ExistingAppsBlock>) {
    lines.push(format!("  - inventory_group: {}", format_scalar(name)));
    match existing {
        Some(block) => {
            lines.push(format!("    apps:{}", block.inline_suffix));
            lines.extend(block.body_lines.iter().cloned());
        }
        None => lines.push("    apps:".to_string()),
    }
}
