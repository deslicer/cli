//! Exit-code contract (issue #220).

use std::process::Command;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn cli_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer"));
    let home = tempfile::tempdir().expect("temp home").keep();
    cmd.env("HOME", home)
        .env_remove("DESLICER_API_TOKEN")
        .env_remove("DESLICER_DEV_TOKEN")
        .env_remove("OBSERVER_API_URL")
        .env_remove("DESLICER_CONFIG_DIR")
        .env_remove("DESLICER_TOKEN_STORE");
    cmd
}

#[test]
fn groups_list_not_logged_in_exits_4() {
    let _guard = ENV_LOCK.lock().unwrap();
    let output = cli_bin()
        .args(["--log-format", "human", "groups", "list"])
        .output()
        .expect("run");
    assert_eq!(
        output.status.code(),
        Some(4),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn top_level_help_points_at_exit_codes_docs() {
    let output = cli_bin().args(["--help"]).output().expect("run --help");
    assert_eq!(output.status.code(), Some(0));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("docs/exit-codes.md"),
        "expected exit-codes docs pointer in help, got:\n{text}"
    );
}
