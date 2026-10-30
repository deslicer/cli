//! Logged-out `auth status` / `auth whoami` contract (issue #236).
//!
//! - `auth status` is a query → exit 0 with logged_in:false on stdout
//! - `auth whoami` is an identity check → exit 4 with the error envelope on stderr

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
        .env_remove("DESLICER_API_TOKEN")
        .env_remove("DESLICER_DEV_TOKEN")
        .env_remove("OBSERVER_API_URL")
        .env_remove("DESLICER_OUTPUT")
        .env_remove("DESLICER_CONFIG_DIR")
        .env_remove("DESLICER_TOKEN_STORE")
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

#[test]
fn auth_status_logged_out_exits_0_with_logged_in_false() {
    let _guard = env_lock();
    let output = cli_bin()
        .args(["--output", "json", "auth", "status"])
        .output()
        .expect("run");
    assert_eq!(
        output.status.code(),
        Some(0),
        "status is a query; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("status JSON on stdout");
    assert_eq!(stdout["logged_in"], false);
    assert_eq!(stdout["identity"], "none");
    assert_eq!(stdout["ok"], false);
}

#[test]
fn auth_whoami_logged_out_exits_4_with_envelope() {
    let _guard = env_lock();
    let output = cli_bin()
        .args(["--output", "json", "auth", "whoami"])
        .output()
        .expect("run");
    assert_eq!(
        output.status.code(),
        Some(4),
        "whoami logged-out must be auth exit 4; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "logged-out whoami must not print non-envelope JSON on stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let v = last_stderr_json(&String::from_utf8_lossy(&output.stderr));
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["error"]["code"], "auth.not_logged_in");
    assert_eq!(v["error"]["kind"], "auth");
    assert_eq!(v["error"]["exit_code"], 4);
    assert_eq!(v["error"]["next_command"], "deslicer auth login");
    assert!(v["error"]["hint"].is_string());
}
