//! Resolve `change plan --target-group` from a UUID or an exact host-group name.

use uuid::Uuid;

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
}
