//! Usage errors on `change plan` must honour `--json` / `--log-format json` (issue #210).

use std::process::Command;

fn cli_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_deslicer"))
}

fn assert_json_usage_error(args: &[&str]) {
    let output = cli_bin().args(args).output().expect("run deslicer");
    assert_eq!(
        output.status.code(),
        Some(2),
        "args={args:?} stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("\"kind\":\"usage\"") || combined.contains("\"kind\": \"usage\""),
        "expected JSON usage envelope, got: {combined}"
    );
    assert!(
        !combined.contains("invalid input:") || combined.contains("\"error\""),
        "plain-text usage should not be the only output: {combined}"
    );
}

#[test]
fn change_plan_destination_json_emits_usage_envelope() {
    assert_json_usage_error(&["change", "plan", "--destination", "acs-stack:x", "--json"]);
}

#[test]
fn change_plan_destination_empty_stack_json_emits_usage_envelope() {
    assert_json_usage_error(&["change", "plan", "--destination", "acs-stack:", "--json"]);
}

#[test]
fn change_plan_from_registry_json_emits_usage_envelope() {
    assert_json_usage_error(&["change", "plan", "--from-registry", "TA-x", "--json"]);
}

#[test]
fn change_plan_destination_log_format_json_emits_usage_envelope() {
    assert_json_usage_error(&[
        "--log-format",
        "json",
        "change",
        "plan",
        "--destination",
        "acs-stack:x",
    ]);
}
