//! Device-session-only commands must exit 4 / auth.device_session_required
//! when only a static API token (or no session) is present (issue #234).

use std::process::Command;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn cli_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer"));
    let home = tempfile::tempdir().expect("temp home").keep();
    cmd.env("HOME", &home)
        .env("DESLICER_API_URL", "http://127.0.0.1:8799")
        .env(
            "OBSERVER_API_URL",
            "http://127.0.0.1:8799/api/cli/observer/",
        )
        .env("DESLICER_API_TOKEN", "fake")
        .env("DESLICER_TOKEN_STORE", "memory")
        .env_remove("DESLICER_DEV_TOKEN")
        .env_remove("DESLICER_CONFIG_DIR")
        .env_remove("DESLICER_OUTPUT")
        .env_remove("CI")
        .env_remove("GITHUB_ACTIONS")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN");
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

fn assert_device_session_required(argv: &[&str]) {
    let output = cli_bin()
        .args(["--output", "json"])
        .args(argv)
        .output()
        .unwrap_or_else(|e| panic!("run {argv:?}: {e}"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(4),
        "expected exit 4 for {argv:?}, got {:?}\nstderr:\n{stderr}",
        output.status.code()
    );
    assert!(
        output.stdout.is_empty(),
        "errors must not go to stdout for {argv:?}: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let v = last_stderr_json(&stderr);
    assert_eq!(v["schema_version"], 1, "{argv:?}");
    assert_eq!(
        v["error"]["code"], "auth.device_session_required",
        "{argv:?}"
    );
    assert_eq!(v["error"]["kind"], "auth", "{argv:?}");
    assert_eq!(v["error"]["exit_code"], 4, "{argv:?}");
    assert_eq!(
        v["error"]["next_command"], "deslicer auth login",
        "{argv:?}"
    );
    assert!(v["error"]["hint"].is_string(), "{argv:?}");
    assert_eq!(v["error"]["retryable"], false, "{argv:?}");
}

#[test]
fn agent_list_with_static_token_exits_4() {
    let _guard = env_lock();
    assert_device_session_required(&["agent", "list"]);
}

#[test]
fn billing_credits_with_static_token_exits_4() {
    let _guard = env_lock();
    assert_device_session_required(&["billing", "credits"]);
}

#[test]
fn models_list_with_static_token_exits_4() {
    let _guard = env_lock();
    assert_device_session_required(&["models", "list"]);
}

#[test]
fn enroll_list_with_static_token_exits_4() {
    let _guard = env_lock();
    assert_device_session_required(&["enroll", "list"]);
}

#[test]
fn worker_instructions_with_static_token_exits_4() {
    let _guard = env_lock();
    assert_device_session_required(&["worker", "instructions"]);
}
