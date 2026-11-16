//! Deprecated command-name warnings (#187).
//!
//! Canonical names are noun-verb and singular (`cloud stack list`, `model list`).
//! Old names stay as hidden clap aliases for two minor versions. This module
//! only prints the notice; parsing still accepts the aliases.

use std::ffi::OsString;

const RENAMED_FOR: &str = "two minor versions";

/// Flags that consume the following argv token (global and common subcommand flags).
const VALUE_FLAGS: &[&str] = &[
    "-o",
    "--output",
    "--log-format",
    "--ci-platform",
    "--timeout",
    "--retries",
    "--deslicer-api-url", // pragma: allowlist secret
    "--observer-api-url",
    "--environment",
    "--installation",
    "--dir",
    "--repo-id",
    "--name",
    "--filter",
    "--limit",
    "--cursor",
    "--status",
    "--since",
    "--target",
    "--targets",
    "--family",
    "--to-version",
    "--captain",
    "--role-group",
    "--host",
    "--group",
    "--app",
    "--plan-id",
    "--command",
    "--idempotency-key",
];

/// Words that can appear in a command path (not flag values).
fn is_command_word(word: &str) -> bool {
    matches!(
        word,
        "model"
            | "models"
            | "environment"
            | "environments"
            | "group"
            | "groups"
            | "cloud"
            | "acs"
            | "stack"
            | "stacks"
            | "credential"
            | "credentials"
            | "observe"
            | "list"
            | "test"
            | "set"
            | "adopt"
            | "upgrade"
            | "plan"
            | "preview"
            | "readiness"
            | "status"
            | "catalog"
            | "show"
            | "remediate"
            | "draft"
            | "analyse"
            | "inventory"
            | "apps"
            | "mapping"
            | "mappings"
            | "change"
            | "plan-matrix"
            | "byok"
            | "doctor"
            | "auth"
            | "repo"
            | "bootstrap"
    )
}

pub fn emit_deprecations(args: &[OsString]) {
    for warning in deprecation_warnings(args) {
        eprintln!("warning: {warning}");
    }
}

pub fn deprecation_warnings(args: &[OsString]) -> Vec<String> {
    warnings_for_prefix(&command_prefix(args))
}

fn command_prefix(args: &[OsString]) -> Vec<String> {
    let mut prefix = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let token = args[i].to_string_lossy();
        if token == "--" {
            break;
        }
        if token.starts_with('-') {
            if token.contains('=') || !takes_value(&token) {
                i += 1;
            } else {
                i += 2;
            }
            continue;
        }
        if !is_command_word(&token) {
            break;
        }
        prefix.push(token.into_owned());
        i += 1;
    }
    prefix
}

fn takes_value(flag: &str) -> bool {
    VALUE_FLAGS.contains(&flag)
}

fn warnings_for_prefix(prefix: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let words: Vec<&str> = prefix.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["acs", "credentials", ..] | ["acs", "credential", ..] => {
            out.push(moved("acs credentials", "cloud stack credential"));
        }
        ["acs", "observe", ..] => out.push(moved("acs observe", "cloud stack observe")),
        ["acs", "stacks", ..] | ["acs", "stack", ..] | ["acs"] => {
            out.push(moved("acs", "cloud stack"));
        }
        ["cloud", "credentials", ..] | ["cloud", "credential", ..] => {
            out.push(moved("cloud credential", "cloud stack credential"));
        }
        ["cloud", "observe", ..] => out.push(moved("cloud observe", "cloud stack observe")),
        ["cloud", "stacks", ..] => out.push(moved("cloud stacks", "cloud stack")),
        ["models", ..] => out.push(renamed("models", "model")),
        ["environments", ..] => out.push(renamed("environments", "environment")),
        ["groups", ..] => out.push(renamed("groups", "group")),
        _ => {}
    }
    if words.contains(&"mappings") {
        out.push(renamed("mappings", "mapping"));
    }
    push_write_move(&words, &mut out);
    out
}

fn push_write_move(words: &[&str], out: &mut Vec<String>) {
    match words {
        ["upgrade", "plan", ..] => out.push(moved("upgrade plan", "change upgrade plan")),
        ["catalog", "plan", ..] => out.push(moved("catalog plan", "change catalog plan")),
        ["remediate", "draft", ..] => out.push(moved("remediate draft", "change remediate draft")),
        ["inventory", "apps", "adopt", ..] => {
            out.push(moved("inventory apps adopt", "change app adopt"));
        }
        ["environment", "mapping", "set", ..]
        | ["environment", "mappings", "set", ..]
        | ["environments", "mapping", "set", ..]
        | ["environments", "mappings", "set", ..] => {
            out.push(moved("environment mapping set", "change mapping set"));
        }
        ["environment", "mapping", "adopt", ..]
        | ["environment", "mappings", "adopt", ..]
        | ["environments", "mapping", "adopt", ..]
        | ["environments", "mappings", "adopt", ..] => {
            out.push(moved("environment mapping adopt", "change mapping adopt"));
        }
        _ => {}
    }
}

fn renamed(old: &str, new: &str) -> String {
    format!(
        "`deslicer {old}` is deprecated and will be removed in {RENAMED_FOR}; use `deslicer {new}`" // pragma: allowlist secret
    )
}

fn moved(old: &str, new: &str) -> String {
    format!(
        "`deslicer {old}` is deprecated and will be removed in {RENAMED_FOR}; use `deslicer {new}`" // pragma: allowlist secret
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warn(args: &[&str]) -> Vec<String> {
        let mut full = vec!["deslicer".into()]; // pragma: allowlist secret
        full.extend(args.iter().map(|s| OsString::from(*s)));
        deprecation_warnings(&full)
    }

    #[test]
    fn canonical_names_are_silent() {
        for args in [
            &["cloud", "stack", "list"][..],
            &["cloud", "stack", "credential", "list"][..],
            &["cloud", "stack", "observe"][..],
            &["model", "list"][..],
            &["environment", "mapping", "list"][..],
            &["group", "list"][..],
            &["change", "upgrade", "plan"][..],
            &["change", "catalog", "plan"][..],
            &["change", "app", "adopt"][..],
            &["change", "mapping", "set"][..],
            &["upgrade", "preview"][..],
            &["catalog", "list"][..],
        ] {
            assert!(warn(args).is_empty(), "{args:?} -> {:?}", warn(args));
        }
    }

    #[test]
    fn aliases_warn_once_with_the_canonical_command() {
        let warnings = warn(&["--output", "json", "acs", "stacks", "list"]);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("cloud stack"));

        let warnings = warn(&["models", "list"]);
        assert!(warnings[0].contains("deslicer model")); // pragma: allowlist secret

        let warnings = warn(&["environments", "mappings", "set", "--environment", "prod"]);
        assert!(warnings.iter().any(|w| w.contains("mapping")));
        assert!(warnings.iter().any(|w| w.contains("change mapping set")));

        let warnings = warn(&["inventory", "apps", "adopt", "TA-nix"]);
        assert!(warnings[0].contains("change app adopt"));
    }

    #[test]
    fn flag_values_are_not_command_words() {
        let warnings = warn(&["cloud", "stack", "list", "--name", "stacks"]);
        assert!(warnings.is_empty(), "{warnings:?}");
    }
}
