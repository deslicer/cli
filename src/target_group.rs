//! Resolve `change plan --target-group` from a UUID, exact host-group name, or
//! the destinations listed in a tenant environment YAML.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::environment_paths::{environment_file_on_disk, search_roots_for};
use crate::environment_yaml::extract_apps_blocks;
use crate::errors::CliError;
use crate::observer_client::HostGroup;

/// True when `spec` is a UUID (Observer host-group id form).
pub fn looks_like_uuid(spec: &str) -> bool {
    Uuid::parse_str(spec.trim()).is_ok()
}

/// Resolve a `--target-group` value to a host-group UUID.
///
/// - UUID-shaped values are returned trimmed (no live lookup).
/// - Otherwise the value is treated as an exact `HostGroup.name` match against
///   `groups` (fail closed on zero or multiple matches).
pub fn resolve_target_group_id(spec: &str, groups: &[HostGroup]) -> Result<String, CliError> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(CliError::Other(
            "--target-group must be a host-group UUID or exact group name".into(),
        ));
    }
    if looks_like_uuid(trimmed) {
        return Ok(trimmed.to_string());
    }
    resolve_group_name(trimmed, groups)
}

fn resolve_group_name(name: &str, groups: &[HostGroup]) -> Result<String, CliError> {
    let matches: Vec<&HostGroup> = groups.iter().filter(|group| group.name == name).collect();
    match matches.as_slice() {
        [only] => Ok(only.id.clone()),
        [] => Err(CliError::Other(format!(
            "no host group named {name:?}. Run `deslicer groups list` and use the \
             ID column, or set --target-group to the exact `name` that matches \
             inventory_group in .deslicer/environments/<env>.yml"
        ))),
        many => Err(CliError::Other(format!(
            "expected exactly one host group named {name:?} (found {}); \
             portal host group names must be unique",
            many.len()
        ))),
    }
}

/// Inventory group names from env YAML that list at least one `source_path`.
///
/// Order follows the file; duplicates are dropped (first wins). Empty `apps:`
/// destinations are skipped — they are placeholders, not plan targets.
pub fn inventory_groups_with_apps(env_yaml: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for block in extract_apps_blocks(env_yaml) {
        if block.source_paths().is_empty() {
            continue;
        }
        if seen.insert(block.group_name.clone()) {
            names.push(block.group_name);
        }
    }
    names
}

/// Choose the `--target-group` spec: explicit flag, else a single env destination.
///
/// Observer currently allows only one active plan per `(tenant, repo, commit)`,
/// so multiple destinations with apps require an explicit `--target-group`
/// (name or UUID). Full desired-vs-observed reconcile for that group is unchanged.
pub fn choose_target_group_spec(
    explicit: Option<&str>,
    env_yaml: Option<&str>,
) -> Result<String, CliError> {
    if let Some(spec) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(spec.to_string());
    }
    let Some(yaml) = env_yaml else {
        return Err(missing_target_group_hint(None));
    };
    let names = inventory_groups_with_apps(yaml);
    match names.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(missing_target_group_hint(Some(
            "environment YAML has no destinations with source_path apps",
        ))),
        many => Err(CliError::Other(format!(
            "environment YAML maps apps to multiple inventory_groups ({}); \
             pass --target-group <name-or-uuid> to select one. Observer allows \
             one active plan per repository commit today, so multi-group fan-out \
             for the same SHA is not supported yet.",
            many.join(", ")
        ))),
    }
}

fn missing_target_group_hint(detail: Option<&str>) -> CliError {
    let suffix = detail.map(|text| format!(" ({text})")).unwrap_or_default();
    CliError::Other(format!(
        "git-sourced `change plan` with DESLICER_API_TOKEN requires \
         --target-group <host-group-uuid-or-name>{suffix}. Pass the \
         inventory_group name from the environment YAML, or run \
         `deslicer groups list` for IDs."
    ))
}

/// Read `.deslicer/environments/<stem>.yml` (or `.yaml`) from the search roots.
pub fn read_environment_yaml(stem: &str) -> Result<Option<String>, CliError> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let roots = search_roots_for(&cwd);
    for root in &roots {
        if let Some(path) = find_environment_file(root, stem) {
            let content = std::fs::read_to_string(&path)
                .map_err(|err| CliError::Other(format!("read {}: {err}", path.display())))?;
            return Ok(Some(content));
        }
    }
    Ok(None)
}

fn find_environment_file(root: &Path, stem: &str) -> Option<PathBuf> {
    let yml = environment_file_on_disk(root, stem);
    if yml.is_file() {
        return Some(yml);
    }
    let yaml = root
        .join(crate::environment_yaml::DESLICER_ENVIRONMENTS_DIR)
        .join(format!("{stem}.yaml"));
    if yaml.is_file() {
        Some(yaml)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: &str, name: &str) -> HostGroup {
        HostGroup {
            id: id.into(),
            name: name.into(),
            display_name: None,
            member_count: None,
        }
    }

    #[test]
    fn uuid_passes_through_without_list() {
        let id = "019f36d6-3f61-7eea-9417-7ac4a8a10f69";
        assert_eq!(resolve_target_group_id(id, &[]).unwrap(), id);
        assert!(looks_like_uuid(id));
    }

    #[test]
    fn uuid_trims_whitespace() {
        let id = "019f36d6-3f61-7eea-9417-7ac4a8a10f69";
        assert_eq!(
            resolve_target_group_id(&format!("  {id}  "), &[]).unwrap(),
            id
        );
    }

    #[test]
    fn name_resolves_to_unique_id() {
        let groups = vec![
            group("11111111-1111-4111-8111-111111111111", "indexers"),
            group("019f36d6-3f61-7eea-9417-7ac4a8a10f69", "search-heads"),
        ];
        assert_eq!(
            resolve_target_group_id("search-heads", &groups).unwrap(),
            "019f36d6-3f61-7eea-9417-7ac4a8a10f69"
        );
    }

    #[test]
    fn name_is_exact_and_case_sensitive() {
        let groups = vec![group(
            "019f36d6-3f61-7eea-9417-7ac4a8a10f69",
            "search-heads",
        )];
        let err = resolve_target_group_id("Search-Heads", &groups).unwrap_err();
        assert!(err.to_string().contains("no host group named"));
    }

    #[test]
    fn display_name_is_not_used() {
        let groups = vec![HostGroup {
            id: "019f36d6-3f61-7eea-9417-7ac4a8a10f69".into(),
            name: "search-heads".into(),
            display_name: Some("Search Heads".into()),
            member_count: Some(2),
        }];
        let err = resolve_target_group_id("Search Heads", &groups).unwrap_err();
        assert!(err.to_string().contains("no host group named"));
    }

    #[test]
    fn missing_name_fails_closed() {
        let err = resolve_target_group_id("missing", &[]).unwrap_err();
        assert!(err.to_string().contains("no host group named \"missing\""));
        assert!(err.to_string().contains("groups list"));
    }

    #[test]
    fn duplicate_names_fail_closed() {
        let groups = vec![
            group("11111111-1111-4111-8111-111111111111", "dup"),
            group("22222222-2222-4222-8222-222222222222", "dup"),
        ];
        let err = resolve_target_group_id("dup", &groups).unwrap_err();
        assert!(err.to_string().contains("expected exactly one"));
        assert!(err.to_string().contains("found 2"));
    }

    #[test]
    fn empty_spec_errors() {
        let err = resolve_target_group_id("   ", &[]).unwrap_err();
        assert!(err.to_string().contains("--target-group must be"));
    }

    #[test]
    fn inventory_groups_skips_empty_apps() {
        let yaml = "destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_nix\n\
\x20\x20- inventory_group: forwarders\n\
\x20\x20\x20\x20apps:\n\
\x20\x20- inventory_group: search_heads\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_windows\n";
        assert_eq!(
            inventory_groups_with_apps(yaml),
            vec!["indexers".to_string(), "search_heads".to_string()]
        );
    }

    #[test]
    fn choose_explicit_wins() {
        let yaml = "destinations:\n  - inventory_group: indexers\n    apps:\n      - source_path: apps/a\n";
        assert_eq!(
            choose_target_group_spec(Some("search-heads"), Some(yaml)).unwrap(),
            "search-heads"
        );
    }

    #[test]
    fn choose_single_destination_from_yaml() {
        let yaml = "destinations:\n  - inventory_group: indexers\n    apps:\n      - source_path: apps/a\n  - inventory_group: empty\n    apps:\n";
        assert_eq!(
            choose_target_group_spec(None, Some(yaml)).unwrap(),
            "indexers"
        );
    }

    #[test]
    fn choose_multiple_destinations_requires_flag() {
        let yaml = "destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/a\n\
\x20\x20- inventory_group: search_heads\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/b\n";
        let err = choose_target_group_spec(None, Some(yaml)).unwrap_err();
        assert!(err.to_string().contains("multiple inventory_groups"));
        assert!(err.to_string().contains("indexers"));
        assert!(err.to_string().contains("search_heads"));
        assert!(err.to_string().contains("one active plan"));
    }

    #[test]
    fn choose_without_yaml_or_flag_errors() {
        let err = choose_target_group_spec(None, None).unwrap_err();
        assert!(err.to_string().contains("--target-group"));
    }
}
