#![allow(clippy::await_holding_lock)]

use std::process::Command;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn cli_bin() // pragma: allowlist secret -> Command {
    Command::new(env!("CARGO_BIN_EXE_deslicer")) // pragma: allowlist secret
}

#[test]
fn auth_status_ci_missing_oidc_exits_nonzero_and_redacts_dev_token() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("DESLICER_DEV_TOKEN", "super-secret-dev-token-value"); // pragma: allowlist secret
    std::env::remove_var("ACTIONS_ID_TOKEN_REQUEST_URL");
    std::env::remove_var("ACTIONS_ID_TOKEN_REQUEST_TOKEN");

    let output = cli_bin()
        .args([
            "auth",
            "status",
            "--ci-platform",
            "github",
            "--log-format",
            "json",
        ])
        .output()
        .expect("run auth status");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        !combined.contains("super-secret-dev-token-value"),
        "secret leaked in output: {combined}"
    );
    assert!(stdout.contains("\"ok\": false") || stdout.contains("\"ok\":false"));
    assert!(stderr.contains("error"));

    std::env::remove_var("DESLICER_DEV_TOKEN"); // pragma: allowlist secret
}

#[test]
fn auth_whoami_ci_missing_oidc_does_not_claim_logged_in() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::remove_var("ACTIONS_ID_TOKEN_REQUEST_URL");
    std::env::remove_var("ACTIONS_ID_TOKEN_REQUEST_TOKEN");

    let output = cli_bin()
        .args([
            "auth",
            "whoami",
            "--ci-platform",
            "github",
            "--log-format",
            "json",
        ])
        .output()
        .expect("run auth whoami");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"logged_in\": false") || stdout.contains("\"logged_in\":false"));
    assert!(!stdout.contains("\"logged_in\": true"));
}

#[test]
fn auth_logout_respects_human_format() {
    let output = cli_bin()
        .args(["auth", "logout", "--log-format", "human"])
        .output()
        .expect("run auth logout");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Logged out"));
    assert!(!stdout.trim_start().starts_with('{'));
}
