//! Machine-readable CLI self-description (`deslicer schema`, `--help --json`).
//!
//! Generated from the clap command tree so agents discover flags without scraping
//! help text (agent-native contract §4 / issue #222).

use clap::{ArgAction, Command, CommandFactory};
use serde::Serialize;
use serde_json::{json, Value};

use crate::cli::Cli;
use crate::errors::PUBLISHED_EXIT_CODES;

const SCHEMA_VERSION: u32 = 1;

/// Exit-code rows republished for agents (mirrors `docs/exit-codes.md`).
const EXIT_CODE_ROWS: &[(i32, &str, bool)] = &[
    (0, "ok (also a no-op or idempotent replay)", false),
    (
        1,
        "unexpected or internal error; multi-part failure without a more specific part code",
        false,
    ),
    (
        2,
        "usage / invalid input / input required / conflicting output flags",
        false,
    ),
    (
        4,
        "auth (not logged in, token rejected, device-login timeout / pending)",
        false,
    ),
    (5, "repository not allowlisted", false),
    (6, "environment not bound", false),
    (7, "ambiguous binding / git-backed change management", false),
    (8, "unsupported CI platform", false),
    (9, "rate limited (honour Retry-After)", true),
    (10, "backend or transport transient", true),
    (11, "plan not found", false),
    (12, "human approval required", false),
    (13, "agent run failed", false),
    (
        14,
        "plan lifecycle failed (change status terminal failure)",
        false,
    ),
    (15, "endpoint not available on this Observer", false),
    (20, "validation blocked", false),
    (
        21,
        "validation warning (treated as failure unless policy says otherwise)",
        false,
    ),
    (22, "validation model unavailable", true),
    (23, "validation timeout", true),
    (24, "validation unavailable", true),
    (130, "cancelled (SIGINT)", false),
];

#[derive(Debug, Serialize)]
struct SchemaFlag {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    short: Option<String>,
    #[serde(rename = "type")]
    type_name: String,
    required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#enum: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    global: bool,
    multiple: bool,
}

#[derive(Debug, Serialize)]
struct SchemaPositional {
    name: String,
    required: bool,
    multiple: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

#[derive(Debug, Serialize)]
struct SchemaCommand {
    path: String,
    summary: String,
    mutating: bool,
    requires_yes: bool,
    paginated: bool,
    flags: Vec<SchemaFlag>,
    positionals: Vec<SchemaPositional>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    subcommands: Vec<String>,
}

/// Build the full schema document (or a filtered view when `command` is set).
pub fn build_schema(command_filter: Option<&str>) -> Result<Value, String> {
    let root = Cli::command();
    let global_flags = collect_flags(&root, /*globals_only*/ true);
    let mut commands = Vec::new();
    walk(&root, "", &mut commands, true);

    if let Some(filter) = command_filter.map(str::trim).filter(|s| !s.is_empty()) {
        let normalized = normalize_path(filter);
        let matched: Vec<_> = commands
            .into_iter()
            .filter(|c| c.path == normalized || c.path.starts_with(&format!("{normalized} ")))
            .collect();
        if matched.is_empty() {
            return Err(format!(
                "unknown command path {filter:?}; pass a clap path like \"change plan\""
            ));
        }
        commands = matched;
    }

    let exit_codes: Vec<Value> = EXIT_CODE_ROWS
        .iter()
        .map(|(code, meaning, retryable)| {
            json!({
                "code": code,
                "meaning": meaning,
                "retryable": retryable,
            })
        })
        .collect();

    debug_assert_eq!(
        EXIT_CODE_ROWS.len(),
        PUBLISHED_EXIT_CODES.len(),
        "schema exit-code rows must stay in sync with PUBLISHED_EXIT_CODES"
    );

    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "cli_version": env!("CARGO_PKG_VERSION"),
        "exit_codes": exit_codes,
        "global_flags": global_flags,
        "commands": commands,
    }))
}

pub fn print_schema(command_filter: Option<&str>) -> i32 {
    match build_schema(command_filter) {
        Ok(doc) => match serde_json::to_string_pretty(&doc) {
            Ok(text) => {
                println!("{text}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize schema: {err}");
                1
            }
        },
        Err(message) => {
            eprintln!("{message}");
            2
        }
    }
}

/// True when argv asks for help *and* JSON self-description (`--help --json`).
pub fn wants_help_json(args: &[impl AsRef<str>]) -> bool {
    let mut help = false;
    let mut json = false;
    let mut i = 0;
    let args: Vec<&str> = args.iter().map(|a| a.as_ref()).collect();
    while i < args.len() {
        let arg = args[i];
        if arg == "--" {
            break;
        }
        if arg == "--help" || arg == "-h" {
            help = true;
            i += 1;
            continue;
        }
        if arg == "--json" {
            json = true;
            i += 1;
            continue;
        }
        if arg == "--output" || arg == "-o" {
            if let Some(value) = args.get(i + 1) {
                if value.eq_ignore_ascii_case("json") {
                    json = true;
                }
                i += 2;
                continue;
            }
        }
        if let Some(value) = arg.strip_prefix("--output=") {
            if value.eq_ignore_ascii_case("json") {
                json = true;
            }
            i += 1;
            continue;
        }
        if let Some(value) = arg.strip_prefix("-o") {
            if !value.is_empty() && value.eq_ignore_ascii_case("json") {
                json = true;
            }
        }
        i += 1;
    }
    help && json
}

/// Command path segments after the binary name, ignoring global flags / help / json.
pub fn help_json_command_path(args: &[impl AsRef<str>]) -> Option<String> {
    let mut path = Vec::new();
    let mut i = 1; // skip argv0
    let args: Vec<&str> = args.iter().map(|a| a.as_ref()).collect();
    while i < args.len() {
        let arg = args[i];
        if arg == "--" {
            break;
        }
        if arg == "--help" || arg == "-h" || arg == "--json" || arg == "--version" || arg == "-V" {
            i += 1;
            continue;
        }
        if arg == "--output"
            || arg == "-o"
            || arg == "--log-format"
            || arg == "--ci-platform"
            || arg == "--deslicer-api-url"
            || arg == "--observer-api-url"
        {
            i += 2;
            continue;
        }
        if arg.starts_with("--output=")
            || arg.starts_with("--log-format=")
            || arg.starts_with("--ci-platform=")
            || arg.starts_with("--deslicer-api-url=")
            || arg.starts_with("--observer-api-url=")
            || (arg.starts_with("-o") && arg != "-o")
        {
            i += 1;
            continue;
        }
        if arg.starts_with('-') {
            if let Some(next) = args.get(i + 1) {
                if !next.starts_with('-') {
                    i += 2;
                    continue;
                }
            }
            i += 1;
            continue;
        }
        path.push(arg.to_string());
        i += 1;
    }
    if path.is_empty() {
        None
    } else {
        Some(path.join(" "))
    }
}

fn normalize_path(raw: &str) -> String {
    raw.split_whitespace()
        .filter(|part| *part != "deslicer")
        .collect::<Vec<_>>()
        .join(" ")
}

fn walk(cmd: &Command, parent_path: &str, out: &mut Vec<SchemaCommand>, is_root: bool) {
    if cmd.get_name() == "help" {
        return;
    }

    if is_root {
        for sub in cmd.get_subcommands() {
            walk(sub, "", out, false);
        }
        return;
    }

    let name = cmd.get_name();
    let path = if parent_path.is_empty() {
        name.to_string()
    } else {
        format!("{parent_path} {name}")
    };

    out.push(describe_command(cmd, &path));

    for sub in cmd.get_subcommands() {
        walk(sub, &path, out, false);
    }
}

fn describe_command(cmd: &Command, path: &str) -> SchemaCommand {
    let summary = cmd
        .get_about()
        .or_else(|| cmd.get_long_about())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let (flags, positionals, has_yes, has_limit, has_cursor) = collect_local_args(cmd);

    let subcommands: Vec<String> = cmd
        .get_subcommands()
        .filter(|s| s.get_name() != "help" && !s.is_hide_set())
        .map(|s| format!("{path} {}", s.get_name()))
        .collect();

    let requires_yes = has_yes;
    let mutating = requires_yes || is_mutating_path(path);
    let paginated = has_limit || has_cursor;

    SchemaCommand {
        path: path.to_string(),
        summary,
        mutating,
        requires_yes,
        paginated,
        flags,
        positionals,
        subcommands,
    }
}

fn collect_flags(cmd: &Command, globals_only: bool) -> Vec<SchemaFlag> {
    let (flags, _, _, _, _) = collect_args(cmd, globals_only);
    flags
}

fn collect_local_args(cmd: &Command) -> (Vec<SchemaFlag>, Vec<SchemaPositional>, bool, bool, bool) {
    collect_args(cmd, false)
}

fn collect_args(
    cmd: &Command,
    globals_only: bool,
) -> (Vec<SchemaFlag>, Vec<SchemaPositional>, bool, bool, bool) {
    let mut flags = Vec::new();
    let mut positionals = Vec::new();
    let mut has_yes = false;
    let mut has_limit = false;
    let mut has_cursor = false;

    for arg in cmd.get_arguments() {
        if arg.is_hide_set() {
            continue;
        }
        let id = arg.get_id().as_str();
        if id == "help" || id == "version" {
            continue;
        }

        if arg.is_positional() {
            if globals_only {
                continue;
            }
            let multiple = matches!(arg.get_action(), ArgAction::Append | ArgAction::Count)
                || arg.get_num_args().is_some_and(|n| n.max_values() > 1);
            positionals.push(SchemaPositional {
                name: id.to_string(),
                required: arg.is_required_set(),
                multiple,
                summary: arg
                    .get_help()
                    .or_else(|| arg.get_long_help())
                    .map(|s| s.to_string()),
            });
            continue;
        }

        if globals_only && !arg.is_global_set() {
            continue;
        }
        // Leaf command listings omit inherited globals (they live under `global_flags`).
        if !globals_only && arg.is_global_set() {
            continue;
        }

        let long = arg.get_long().map(|s| s.to_string());
        let short = arg.get_short().map(|c| format!("-{c}"));
        let name = long
            .clone()
            .map(|s| format!("--{s}"))
            .or_else(|| short.clone())
            .unwrap_or_else(|| format!("--{id}"));

        if long.as_deref() == Some("yes") {
            has_yes = true;
        }
        if long.as_deref() == Some("limit") {
            has_limit = true;
        }
        if long.as_deref() == Some("cursor") {
            has_cursor = true;
        }

        let enum_values: Option<Vec<String>> = {
            let vals: Vec<String> = arg
                .get_possible_values()
                .iter()
                .map(|v| v.get_name().to_string())
                .collect();
            if vals.is_empty() {
                None
            } else {
                Some(vals)
            }
        };

        let default = arg
            .get_default_values()
            .first()
            .and_then(|os| os.to_str().map(|s| s.to_string()));

        let env = arg
            .get_env()
            .and_then(|os| os.to_str().map(|s| s.to_string()));

        let multiple = matches!(arg.get_action(), ArgAction::Append | ArgAction::Count)
            || arg.get_num_args().is_some_and(|n| n.max_values() > 1);

        flags.push(SchemaFlag {
            name,
            short,
            type_name: flag_type(arg),
            required: arg.is_required_set(),
            default,
            r#enum: enum_values,
            env,
            summary: arg
                .get_help()
                .or_else(|| arg.get_long_help())
                .map(|s| s.to_string()),
            global: arg.is_global_set(),
            multiple,
        });
    }

    (flags, positionals, has_yes, has_limit, has_cursor)
}

fn flag_type(arg: &clap::Arg) -> String {
    match arg.get_action() {
        ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Help | ArgAction::Version => {
            "boolean".into()
        }
        ArgAction::Count => "count".into(),
        _ => {
            if !arg.get_possible_values().is_empty() {
                "enum".into()
            } else {
                "string".into()
            }
        }
    }
}

/// Draft-create and other side-effecting paths that do not require `--yes`.
fn is_mutating_path(path: &str) -> bool {
    const MUTATING_SUFFIXES: &[&str] = &[
        "change plan",
        "change approve",
        "change deploy",
        "catalog plan",
        "upgrade plan",
        "remediate draft",
        "enroll create",
        "enroll revoke",
        "license bind",
        "models byok set",
        "org switch",
        "org settings approval",
        "org settings execution",
        "agent clone",
        "agent run",
        "init",
        "repo bootstrap",
        "repo refresh",
        "skill import",
        "routine enable",
        "routine disable",
        "routine run",
        "inventory sync",
        "inventory import-seed",
        "auth login",
        "auth logout",
        "update",
    ];
    MUTATING_SUFFIXES.iter().any(|s| path == *s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_lists_change_plan_and_exit_codes() {
        let doc = build_schema(None).expect("schema");
        assert_eq!(doc["schema_version"], 1);
        assert!(doc["exit_codes"].as_array().unwrap().len() >= 20);
        let commands = doc["commands"].as_array().unwrap();
        assert!(
            commands.iter().any(|c| c["path"] == "change plan"),
            "missing change plan"
        );
        let plan = commands
            .iter()
            .find(|c| c["path"] == "change plan")
            .unwrap();
        assert_eq!(plan["mutating"], true);
        assert_eq!(plan["requires_yes"], false);
    }

    #[test]
    fn schema_filter_unknown_path_errors() {
        let err = build_schema(Some("not a real command")).unwrap_err();
        assert!(err.contains("unknown command path"));
    }

    #[test]
    fn schema_filter_narrows_to_prefix() {
        let doc = build_schema(Some("change plan")).expect("schema");
        let commands = doc["commands"].as_array().unwrap();
        assert!(!commands.is_empty());
        assert!(commands.iter().all(|c| {
            let p = c["path"].as_str().unwrap();
            p == "change plan" || p.starts_with("change plan ")
        }));
    }

    #[test]
    fn requires_yes_detected_on_gated_apply() {
        let doc = build_schema(Some("change revert")).expect("schema");
        let revert = doc["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["path"] == "change revert")
            .expect("revert");
        assert_eq!(revert["requires_yes"], true);
        assert_eq!(revert["mutating"], true);

        let approve_doc = build_schema(Some("change approve")).expect("schema");
        let approve = approve_doc["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["path"] == "change approve")
            .expect("approve");
        // Approve mutates without a local `--yes` gate (CI proxy / portal attest).
        assert_eq!(approve["requires_yes"], false);
        assert_eq!(approve["mutating"], true);
    }

    #[test]
    fn wants_help_json_detects_flag_combos() {
        assert!(wants_help_json(&["deslicer", "--help", "--json"]));
        assert!(wants_help_json(&["deslicer", "--json", "--help"]));
        assert!(wants_help_json(&[
            "deslicer", "--output", "json", "auth", "--help"
        ]));
        assert!(!wants_help_json(&["deslicer", "--help"]));
        assert!(!wants_help_json(&["deslicer", "--json", "auth", "status"]));
    }

    #[test]
    fn help_json_command_path_strips_flags() {
        assert_eq!(
            help_json_command_path(&["deslicer", "--help", "--json"]),
            None
        );
        assert_eq!(
            help_json_command_path(&["deslicer", "change", "plan", "--help", "--json"]),
            Some("change plan".into())
        );
        assert_eq!(
            help_json_command_path(&["deslicer", "--output", "json", "auth", "login", "--help"]),
            Some("auth login".into())
        );
    }
}
