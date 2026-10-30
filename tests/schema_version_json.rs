//! Success JSON carries `schema_version` (issue #223).

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

#[test]
fn auth_whoami_json_includes_schema_version() {
    let _guard = env_lock();
    let output = cli_bin()
        .args([
            "--ci-platform",
            "local",
            "--output",
            "json",
            "auth",
            "whoami",
        ])
        .output()
        .expect("run");
    // Logged-out whoami may be exit 4 with envelope on stderr; when JSON lands
    // on stdout for a success path, require schema_version. Prefer stderr envelope.
    if output.status.code() == Some(0) {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let doc: Value = serde_json::from_str(stdout.trim()).expect("stdout json");
        assert_eq!(doc["schema_version"], 1);
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap();
        let doc: Value = serde_json::from_str(line).expect("stderr envelope");
        assert_eq!(doc["schema_version"], 1);
    }
}
