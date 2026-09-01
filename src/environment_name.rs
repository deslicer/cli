//! Canonical Observer `environment_name` derivation.
//!
//! Byte-for-byte port of DAP `observer-api/src/github_repo_sync/environment_name.rs`
//! (itself a port of DAI `deriveEnvironmentName`). The implementations must stay
//! in lockstep so Path A2 files match App-provisioned stems.

/// Fallback when the input cannot be sanitised into a valid name.
pub const DEFAULT_ENVIRONMENT_NAME: &str = "default";

/// Mirrors DAI's `MAX_ENVIRONMENT_NAME_LENGTH`.
const MAX_ENVIRONMENT_NAME_LENGTH: usize = 255;

/// Derive the canonical Observer `environment_name` from a tenant slug (or any
/// free-form label).
///
/// 1. trim whitespace
/// 2. replace every char outside `[A-Za-z0-9_-]` with `-`
/// 3. collapse runs of `-` into a single `-`
/// 4. strip the leading run of non-letters
/// 5. truncate to 255 chars
/// 6. strip the trailing run of `-`/`_`
/// 7. fall back to `"default"` when the result fails the Observer regex
pub fn derive_environment_name(input: &str) -> String {
    let replaced: String = input
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect();

    let mut collapsed = String::with_capacity(replaced.len());
    for ch in replaced.chars() {
        if ch == '-' && collapsed.ends_with('-') {
            continue;
        }
        collapsed.push(ch);
    }

    let stripped = collapsed.trim_start_matches(|ch: char| !ch.is_ascii_alphabetic());
    let truncated: String = stripped.chars().take(MAX_ENVIRONMENT_NAME_LENGTH).collect();
    let cleaned = truncated.trim_end_matches(['-', '_']);

    if is_valid_environment_name(cleaned) {
        cleaned.to_string()
    } else {
        DEFAULT_ENVIRONMENT_NAME.to_string()
    }
}

/// Observer environment-name contract: starts with a letter, ends with an
/// alphanumeric, interior allows `_` and `-`.
pub fn is_valid_environment_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let Some(last) = name.chars().next_back() else {
        return false;
    };
    if name.len() > 1 && !last.is_ascii_alphanumeric() {
        return false;
    }
    name.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::{derive_environment_name, is_valid_environment_name};

    #[test]
    fn passes_through_already_valid_slugs() {
        assert_eq!(derive_environment_name("acme-prod"), "acme-prod");
        assert_eq!(derive_environment_name("a"), "a");
        assert_eq!(derive_environment_name("Acme_Prod-1"), "Acme_Prod-1");
    }

    #[test]
    fn replaces_invalid_chars_and_collapses_hyphens() {
        assert_eq!(derive_environment_name("acme prod!"), "acme-prod");
        assert_eq!(derive_environment_name("acme...prod"), "acme-prod");
        assert_eq!(derive_environment_name("a__b"), "a__b");
    }

    #[test]
    fn trims_whitespace_before_sanitising() {
        assert_eq!(derive_environment_name("  Acme Prod!  "), "Acme-Prod");
    }

    #[test]
    fn strips_leading_non_letters() {
        assert_eq!(derive_environment_name("123-prod"), "prod");
        assert_eq!(derive_environment_name("_-_prod"), "prod");
    }

    #[test]
    fn strips_trailing_hyphens_and_underscores() {
        assert_eq!(derive_environment_name("prod-"), "prod");
        assert_eq!(derive_environment_name("prod_-_"), "prod");
    }

    #[test]
    fn falls_back_to_default_when_unsalvageable() {
        assert_eq!(derive_environment_name(""), "default");
        assert_eq!(derive_environment_name("   "), "default");
        assert_eq!(derive_environment_name("123"), "default");
        assert_eq!(derive_environment_name("---"), "default");
        assert_eq!(derive_environment_name("!!!"), "default");
    }

    #[test]
    fn truncates_to_255_chars_before_trailing_strip() {
        let long = "a".repeat(300);
        assert_eq!(derive_environment_name(&long), "a".repeat(255));

        let mut input = "a".repeat(254);
        input.push('-');
        input.push_str(&"b".repeat(40));
        assert_eq!(derive_environment_name(&input), "a".repeat(254));
    }

    #[test]
    fn non_ascii_becomes_hyphen() {
        assert_eq!(derive_environment_name("héllo"), "h-llo");
        assert_eq!(derive_environment_name("æøå"), "default");
    }

    #[test]
    fn validity_matches_observer_regex() {
        for valid in ["a", "ab", "a1", "a_b", "a-b", "A0-z_9"] {
            assert!(is_valid_environment_name(valid), "{valid} should be valid");
        }
        for invalid in ["", "1a", "-a", "a-", "a_", "a b", "a.b", "_"] {
            assert!(
                !is_valid_environment_name(invalid),
                "{invalid} should be invalid"
            );
        }
    }
}
