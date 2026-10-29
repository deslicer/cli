//! Agent-native error envelope + `--output` (issue #221).

use std::process::Command;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn cli_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer"));
    // Empty HOME so we never pick up a real device session and hang on the network.
    let home = tempfile::tempdir().expect("temp home").keep();
    cmd.env("HOME", home)
        .env_remove("DESLICER_API_TOKEN")
        .env_remove("DESLICER_DEV_TOKEN")
        .env_remove("OBSERVER_API_URL")
        .env_remove("DESLICER_OUTPUT")
        .env_remove("CI")
        .env_remove("DESLICER_CONFIG_DIR")
        .env_remove("DESLICER_TOKEN_STORE");
    cmd
}

fn last_stderr_json(stderr: &str) -> serde_json::Value {
    let line = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(stderr);
    serde_json::from_str(line).unwrap_or_else(|e| {
        panic!("last stderr line is not JSON ({e}): {line:?}\nfull stderr:\n{stderr}")
    })
}

#[test]
fn output_json_flag_accepted() {
    let _guard = ENV_LOCK.lock().unwrap();
    let output = cli_bin()
        .args(["--output", "json", "groups", "list"])
        .output()
        .expect("run");
    assert_ne!(
        output.status.code(),
        Some(2),
        "unexpected argument --output"
    );
    assert!(output.stdout.is_empty(), "errors must not go to stdout");
    let v = last_stderr_json(&String::from_utf8_lossy(&output.stderr));
    assert_eq!(v["schema_version"], 1);
    assert!(v["error"]["code"].is_string());
}

#[test]
fn short_o_json_flag_accepted() {
    let _guard = ENV_LOCK.lock().unwrap();
    let output = cli_bin()
        .args(["-o", "json", "groups", "list"])
        .output()
        .expect("run");
    assert_ne!(output.status.code(), Some(2));
    let v = last_stderr_json(&String::from_utf8_lossy(&output.stderr));
    assert_eq!(v["error"]["kind"], "auth");
}

#[test]
fn non_tty_defaults_to_json_error_envelope() {
    let _guard = ENV_LOCK.lock().unwrap();
    // Command::output() pipes stdout → not a TTY; must not require --json/--log-format.
    let output = cli_bin().args(["groups", "list"]).output().expect("run");
    assert!(output.stdout.is_empty());
    let v = last_stderr_json(&String::from_utf8_lossy(&output.stderr));
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["error"]["code"], "auth.not_logged_in");
    assert_eq!(v["error"]["kind"], "auth");
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not logged in"));
    assert!(v["error"]["hint"].is_string());
    assert_eq!(v["error"]["next_command"], "deslicer auth login");
    assert_eq!(v["error"]["retryable"], false);
    assert!(v["error"]["exit_code"].as_i64().is_some());
    assert!(v["error"]["details"].is_object());
}

#[test]
fn deslicer_output_human_overrides_non_tty_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let output = cli_bin()
        .args(["groups", "list"])
        .env("DESLICER_OUTPUT", "human")
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not logged in"),
        "expected human text, got: {stderr}"
    );
    assert!(
        !stderr.trim_start().starts_with('{'),
        "DESLICER_OUTPUT=human must not emit JSON envelope by default"
    );
}

#[test]
fn conflicting_output_flags_exit_2() {
    let _guard = ENV_LOCK.lock().unwrap();
    let output = cli_bin()
        .args(["--output", "human", "--json", "groups", "list"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn clap_usage_error_uses_envelope_with_output_json() {
    let _guard = ENV_LOCK.lock().unwrap();
    let output = cli_bin()
        .args(["--output", "json", "change", "approve"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2));
    let v = last_stderr_json(&String::from_utf8_lossy(&output.stderr));
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["error"]["kind"], "usage");
    assert_eq!(v["error"]["code"], "usage.invalid");
    assert_eq!(v["error"]["exit_code"], 2);
    assert_eq!(v["error"]["retryable"], false);
    assert!(v["error"]["next_command"].is_null());
}
