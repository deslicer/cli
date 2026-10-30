//! Precondition / missing-flag errors must exit 2 / usage.missing_argument
//! (issue #235), not exit 1 / error.other.

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

fn assert_missing_argument(mut cmd: Command, hint_substr: &str) {
    let output = cmd.output().expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2, got {:?}\nstderr:\n{stderr}",
        output.status.code()
    );
    assert!(
        output.stdout.is_empty(),
        "errors must not go to stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let v = last_stderr_json(&stderr);
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["error"]["code"], "usage.missing_argument");
    assert_eq!(v["error"]["kind"], "usage");
    assert_eq!(v["error"]["exit_code"], 2);
    assert_eq!(v["error"]["retryable"], false);
    let hint = v["error"]["hint"].as_str().unwrap_or("");
    assert!(
        hint.contains(hint_substr),
        "hint {hint:?} should mention {hint_substr:?}\nmessage={}",
        v["error"]["message"]
    );
}

#[test]
fn init_without_git_remote_exits_2() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("tmpdir");
    // Not a git repo → no origin remote.
    assert_missing_argument(
        {
            let mut cmd = cli_bin();
            cmd.current_dir(dir.path())
                .args(["--output", "json", "init"]);
            cmd
        },
        "--provider",
    );
}

#[test]
fn agent_run_without_prompt_exits_2() {
    let _guard = env_lock();
    // Force a TTY-like "no stdin prompt" path by passing empty argv; piped
    // stdin is non-TTY so resolve_prompt would read it. Use /dev/tty via
    // `.stdin(Stdio::null())` is still non-TTY for is_terminal(). The
    // no-arg + terminal check needs a real TTY; instead assert the empty
    // whitespace prompt path which shares MissingArgument.
    assert_missing_argument(
        {
            let mut cmd = cli_bin();
            cmd.args(["--output", "json", "agent", "run", "   "]);
            cmd
        },
        "prompt",
    );
}

#[test]
fn agent_repl_non_tty_without_prompt_exits_2() {
    let _guard = env_lock();
    assert_missing_argument(
        {
            let mut cmd = cli_bin();
            cmd.args(["--output", "json", "agent", "repl"])
                .stdin(std::process::Stdio::null());
            cmd
        },
        "prompt",
    );
}

#[test]
fn agent_resume_non_tty_without_prompt_exits_2() {
    let _guard = env_lock();
    // Usage must win over auth (no token) so agents see exit 2, not 4.
    assert_missing_argument(
        {
            let mut cmd = cli_bin();
            cmd.args(["--output", "json", "agent", "resume"])
                .stdin(std::process::Stdio::null());
            cmd
        },
        "prompt",
    );
}

#[test]
fn inventory_validate_without_environment_exits_2() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("tmpdir");
    assert_missing_argument(
        {
            let mut cmd = cli_bin();
            cmd.current_dir(dir.path())
                .args(["--output", "json", "inventory", "validate"]);
            cmd
        },
        "--environment",
    );
}

#[test]
fn change_plan_token_without_target_group_exits_2() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("tmpdir");
    // Empty dir, static token, no env YAML → missing --target-group.
    assert_missing_argument(
        {
            let mut cmd = cli_bin();
            cmd.current_dir(dir.path())
                .env("DESLICER_API_TOKEN", "fake")
                .env(
                    "OBSERVER_API_URL",
                    "http://127.0.0.1:8799/api/cli/observer/",
                )
                .args(["--output", "json", "change", "plan"]);
            cmd
        },
        "--target-group",
    );
}
