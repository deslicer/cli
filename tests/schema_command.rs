//! `deslicer schema` and `--help --json` (issue #222).

use std::process::Command;
use std::sync::Mutex;

use serde_json::Value;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn cli_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer"));
    let home = tempfile::tempdir().expect("temp home").keep();
    cmd.env("HOME", home)
        .env_remove("DESLICER_API_TOKEN")
        .env_remove("DESLICER_OUTPUT")
        .env_remove("CI");
    cmd
}

fn stdout_json(output: &std::process::Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn schema_emits_json_with_commands_and_exit_codes() {
    let _guard = env_lock();
    let output = cli_bin().args(["schema"]).output().expect("run");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let doc = stdout_json(&output);
    assert_eq!(doc["schema_version"], 1);
    assert!(doc["cli_version"].as_str().is_some());
    assert!(doc["exit_codes"].as_array().unwrap().len() >= 20);
    let globals = doc["global_flags"].as_array().unwrap();
    assert!(
        globals.iter().any(|f| f["name"] == "--output"),
        "missing global --output"
    );
    let commands = doc["commands"].as_array().unwrap();
    assert!(commands.iter().any(|c| c["path"] == "change plan"));
    assert!(commands.iter().any(|c| c["path"] == "schema"));
}

#[test]
fn schema_command_filter() {
    let _guard = env_lock();
    let output = cli_bin()
        .args(["schema", "--command", "change revert"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let doc = stdout_json(&output);
    let commands = doc["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["path"], "change revert");
    assert_eq!(commands[0]["requires_yes"], true);
    assert_eq!(commands[0]["mutating"], true);
}

#[test]
fn schema_positional_path_filter() {
    let _guard = env_lock();
    let output = cli_bin()
        .args(["schema", "change", "revert"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let doc = stdout_json(&output);
    let commands = doc["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["path"], "change revert");
}

#[test]
fn schema_positional_group_prefix() {
    let _guard = env_lock();
    let output = cli_bin().args(["schema", "change"]).output().expect("run");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let doc = stdout_json(&output);
    let commands = doc["commands"].as_array().unwrap();
    assert!(commands.len() > 1, "expected all change/* commands");
    assert!(commands.iter().all(|c| {
        let p = c["path"].as_str().unwrap();
        p == "change" || p.starts_with("change ")
    }));
    assert!(commands.iter().any(|c| c["path"] == "change plan"));
}

#[test]
fn schema_unknown_command_exits_2() {
    let _guard = env_lock();
    let output = cli_bin()
        .args(["--output", "json", "schema", "--command", "nope"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap();
    let err: Value = serde_json::from_str(line).expect("envelope");
    assert_eq!(err["error"]["kind"], "usage");
    assert_eq!(err["error"]["exit_code"], 2);
}

#[test]
fn help_json_is_schema_alias() {
    let _guard = env_lock();
    let output = cli_bin().args(["--help", "--json"]).output().expect("run");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let doc = stdout_json(&output);
    assert_eq!(doc["schema_version"], 1);
    assert!(doc["commands"].as_array().unwrap().len() > 10);
}

#[test]
fn help_json_scoped_to_subcommand() {
    let _guard = env_lock();
    let output = cli_bin()
        .args(["change", "plan", "--help", "--json"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let doc = stdout_json(&output);
    let commands = doc["commands"].as_array().unwrap();
    assert!(commands.iter().all(|c| {
        let p = c["path"].as_str().unwrap();
        p == "change plan" || p.starts_with("change plan ")
    }));
}
