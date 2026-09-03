//! Generate and merge `.deslicer/environments/<tenant-slug>.yml`.
//!
//! Shape matches Observer / DAI. Removal rules differ: the CLI keeps
//! destinations that still list `source_path` apps.

mod generate;
mod merge;
mod parse;
mod validate;

pub use generate::{environment_config_file_path, DESLICER_ENVIRONMENTS_DIR};
pub use merge::{
    generate_environment_yaml, merge_environment_yaml, BlockedDestination, MergedEnvironmentYaml,
};
pub use parse::{extract_apps_blocks, ExistingAppsBlock};
pub use validate::{
    resolve_env_file, validate_environment_yaml, Severity, ValidationIssue, ValidationReport,
    VALID_DEST_DIRS, VALID_STATES,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn groups(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn writes_destinations_block_for_every_host_group() {
        let result = generate_environment_yaml(
            "daniel-test",
            "Daniel test",
            &groups(&[
                "AIO_5",
                "all_in_one_servers_ag1",
                "all_in_one_servers_ag2",
                "all_in_one_servers_ag3",
            ]),
        );

        assert_eq!(result.path, ".deslicer/environments/daniel-test.yml");
        assert!(!result.host_group_placeholder);
        assert!(result.content.contains("destinations:"));
        assert!(result
            .content
            .contains("  - inventory_group: AIO_5\n    apps:"));
        assert!(result
            .content
            .contains("  - inventory_group: all_in_one_servers_ag1\n    apps:"));
        assert!(result
            .content
            .contains("  - inventory_group: all_in_one_servers_ag3\n    apps:"));
    }

    #[test]
    fn byte_exact_output_shape() {
        let result = generate_environment_yaml("prod", "Acme", &groups(&["indexers"]));
        let expected = "# Deslicer environment configuration.\n\
# File stem \"prod\" maps to a workspace environment (tenant: Acme).\n\
# Add apps under each inventory_group as `- source_path: <relative-app-path>`.\n\
# See README.md at the repository root for how this file is used.\n\
\n\
destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n";
        assert_eq!(result.content, expected);
    }

    #[test]
    fn dedupes_repeated_names_and_preserves_order() {
        let result = generate_environment_yaml(
            "acme-prod",
            "Acme Prod",
            &groups(&["forwarders", "forwarders", "indexers"]),
        );
        assert_eq!(
            result
                .content
                .matches("inventory_group: forwarders")
                .count(),
            1
        );
        let forwarders_idx = result.content.find("forwarders");
        let indexers_idx = result.content.find("indexers");
        assert!(forwarders_idx < indexers_idx);
    }

    #[test]
    fn quotes_non_plain_scalars() {
        let result = generate_environment_yaml("edge", "Edge", &groups(&["group with spaces"]));
        assert!(result
            .content
            .contains("  - inventory_group: 'group with spaces'\n    apps:"));
    }

    #[test]
    fn emits_placeholder_when_no_groups() {
        let result = generate_environment_yaml("staging", "Staging", &[]);
        assert_eq!(result.path, ".deslicer/environments/staging.yml");
        assert!(result.content.contains("destinations: []"));
        assert!(result.content.contains("TODO:"));
        assert!(result.host_group_placeholder);
    }

    #[test]
    fn merge_adds_group_without_touching_existing_apps() {
        let existing = "# old header\n\
destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_nix\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_windows\n\
\x20\x20- inventory_group: forwarders\n\
\x20\x20\x20\x20apps:\n";
        let result = merge_environment_yaml(
            "prod",
            "Acme",
            &groups(&["indexers", "forwarders", "search_heads"]),
            Some(existing),
        );
        assert!(result.content.contains(
            "  - inventory_group: indexers\n    apps:\n      - source_path: apps/ta_nix\n      - source_path: apps/ta_windows\n"
        ));
        assert!(result
            .content
            .contains("  - inventory_group: forwarders\n    apps:\n"));
        assert!(result
            .content
            .contains("  - inventory_group: search_heads\n    apps:\n"));
        assert_eq!(result.added, vec!["search_heads".to_string()]);
        assert!(result.removed_empty.is_empty());
        assert!(result.blocked.is_empty());
    }

    #[test]
    fn merge_drops_empty_removed_groups() {
        let existing = "destinations:\n\
\x20\x20- inventory_group: legacy_empty\n\
\x20\x20\x20\x20apps:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n";
        let result = merge_environment_yaml("prod", "Acme", &groups(&["indexers"]), Some(existing));
        assert!(!result.content.contains("legacy_empty"));
        assert_eq!(result.removed_empty, vec!["legacy_empty".to_string()]);
        assert!(result.blocked.is_empty());
    }

    #[test]
    fn merge_keeps_removed_group_that_still_has_apps() {
        let existing = "destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_nix\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/ta_windows\n\
\x20\x20- inventory_group: forwarders\n\
\x20\x20\x20\x20apps:\n";
        let result =
            merge_environment_yaml("prod", "Acme", &groups(&["forwarders"]), Some(existing));
        assert!(result.content.contains(
            "  - inventory_group: indexers\n    apps:\n      - source_path: apps/ta_nix\n      - source_path: apps/ta_windows\n"
        ));
        assert_eq!(result.blocked.len(), 1);
        assert_eq!(result.blocked[0].inventory_group, "indexers");
        assert_eq!(
            result.blocked[0].source_paths,
            vec!["apps/ta_nix".to_string(), "apps/ta_windows".to_string()]
        );
        assert!(result.removed_empty.is_empty());
    }

    #[test]
    fn merge_matches_quoted_group_names() {
        let existing = "destinations:\n\
\x20\x20- inventory_group: 'group with spaces'\n\
\x20\x20\x20\x20apps:\n\
\x20\x20\x20\x20\x20\x20- source_path: apps/spaced\n";
        let result = merge_environment_yaml(
            "edge",
            "Edge",
            &groups(&["group with spaces"]),
            Some(existing),
        );
        assert!(result.content.contains(
            "  - inventory_group: 'group with spaces'\n    apps:\n      - source_path: apps/spaced\n"
        ));
    }

    #[test]
    fn merge_preserves_inline_empty_list_suffix() {
        let existing = "destinations:\n\
\x20\x20- inventory_group: indexers\n\
\x20\x20\x20\x20apps: []\n";
        let result = merge_environment_yaml("prod", "Acme", &groups(&["indexers"]), Some(existing));
        assert!(result
            .content
            .contains("  - inventory_group: indexers\n    apps: []\n"));
    }

    #[test]
    fn merged_regeneration_is_idempotent() {
        let first = generate_environment_yaml("prod", "Acme", &groups(&["indexers", "forwarders"]));
        let second = merge_environment_yaml(
            "prod",
            "Acme",
            &groups(&["indexers", "forwarders"]),
            Some(&first.content),
        );
        assert_eq!(first.content, second.content);
        assert!(second.added.is_empty());
    }
}
