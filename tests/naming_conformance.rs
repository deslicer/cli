//! Naming conformance (#187): canonical commands, hidden aliases, doctor.

use std::process::Command;

use clap::Parser;
use deslicer_cli::cli::{Cli, Command as CliCommand}; // pragma: allowlist secret
use deslicer_cli::commands::catalog::CatalogCmd; // pragma: allowlist secret
use deslicer_cli::commands::change::ChangeCmd; // pragma: allowlist secret

fn bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer")); // pragma: allowlist secret
    cmd.env_remove("DESLICER_OUTPUT") // pragma: allowlist secret
        .env_remove("DESLICER_API_TOKEN") // pragma: allowlist secret
        .env_remove("CI")
        .env("DESLICER_TOKEN_STORE", "file") // pragma: allowlist secret
        .env("HOME", std::env::temp_dir());
    cmd
}

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["deslicer"]; // pragma: allowlist secret
    full.extend_from_slice(args);
    Cli::try_parse_from(full).unwrap_or_else(|err| panic!("parse {args:?}: {err}"))
}

#[test]
fn canonical_cloud_stack_and_acs_alias_parse() {
    assert!(matches!(
        parse(&["cloud", "stack", "list"]).command,
        CliCommand::Cloud(_)
    ));
    assert!(matches!(
        parse(&["acs", "stacks", "list"]).command,
        CliCommand::Cloud(_)
    ));
    assert!(matches!(
        parse(&["acs", "credentials", "list"]).command,
        CliCommand::Cloud(_)
    ));
    assert!(matches!(
        parse(&["cloud", "stack", "credential", "list"]).command,
        CliCommand::Cloud(_)
    ));
}

#[test]
fn singular_nouns_and_plural_aliases_parse() {
    assert!(matches!(
        parse(&["model", "list"]).command,
        CliCommand::Model(_)
    ));
    assert!(matches!(
        parse(&["models", "list"]).command,
        CliCommand::Model(_)
    ));
    assert!(matches!(
        parse(&["group", "list"]).command,
        CliCommand::Group(_)
    ));
    assert!(matches!(
        parse(&["groups", "list"]).command,
        CliCommand::Group(_)
    ));
    assert!(matches!(
        parse(&["environment", "mapping", "list", "--environment", "prod"]).command,
        CliCommand::Environment(_)
    ));
    assert!(matches!(
        parse(&["environments", "mappings", "list", "--environment", "prod"]).command,
        CliCommand::Environment(_)
    ));
}

#[test]
fn change_gated_writes_parse_alongside_old_paths() {
    assert!(matches!(
        parse(&[
            "change",
            "upgrade",
            "plan",
            "--family",
            "upgrade_non_clustered",
            "--target",
            "h",
            "--to-version",
            "9.4.1",
        ])
        .command,
        CliCommand::Change(ChangeCmd::Upgrade(_))
    ));
    assert!(matches!(
        parse(&["change", "catalog", "plan", "--target", "indexers", "bp"]).command,
        CliCommand::Change(ChangeCmd::Catalog(_))
    ));
    let cli = parse(&[
        "catalog",
        "plan",
        "bp",
        "--targets",
        "indexers,search_heads",
    ]);
    let CliCommand::Catalog(CatalogCmd::Plan(args)) = cli.command else {
        panic!("catalog plan");
    };
    assert_eq!(args.targets, vec!["indexers", "search_heads"]);
}

#[test]
fn deprecated_alias_warns_on_stderr_and_still_runs_help() {
    let out = bin().args(["models", "--help"]).output().expect("run");
    assert_eq!(out.status.code(), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("deprecated"), "{err}");
    assert!(err.contains("deslicer model"), "{err}"); // pragma: allowlist secret
}

#[test]
fn doctor_without_a_session_is_ready() {
    let home = tempfile::tempdir().expect("home");
    let out = bin()
        .env("HOME", home.path())
        .args(["--output", "json", "doctor"])
        .output()
        .expect("run");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["ok"], true);
    let checks = doc["checks"].as_array().unwrap();
    assert!(checks
        .iter()
        .any(|c| c["id"] == "api_url" && c["ok"] == true));
}

#[test]
fn doctor_missing_workflows_is_not_ready() {
    let home = tempfile::tempdir().expect("home");
    let dir = tempfile::tempdir().expect("dir");
    let out = bin()
        .env("HOME", home.path())
        .args([
            "--output",
            "json",
            "doctor",
            "--dir",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["next_command"], serde_json::Value::Null);
}
