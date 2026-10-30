//! Global `--no-input` / `--non-interactive` / `DESLICER_NO_INPUT` (issue #227).

use clap::CommandFactory;
use deslicer_cli::cli::Cli;
use std::process::Command;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn top_level_help_documents_no_input() {
    let mut buf = Vec::new();
    Cli::command()
        .write_long_help(&mut buf)
        .expect("write help");
    let help = String::from_utf8(buf).expect("utf8");
    assert!(
        help.contains("--no-input"),
        "top-level help missing --no-input:\n{help}"
    );
    assert!(
        help.contains("DESLICER_NO_INPUT") || help.contains("non-interactive"),
        "top-level help should mention env or non-interactive alias:\n{help}"
    );
}

#[test]
fn auth_login_help_documents_no_wait() {
    let mut cmd = Cli::command();
    let auth = cmd.find_subcommand_mut("auth").expect("auth");
    let login = auth.find_subcommand_mut("login").expect("login");
    let mut buf = Vec::new();
    login.write_long_help(&mut buf).expect("help");
    let help = String::from_utf8(buf).expect("utf8");
    assert!(help.contains("--no-wait"), "login help missing --no-wait");
    assert!(help.contains("--wait"), "login help missing --wait");
}

#[test]
fn no_input_flag_is_accepted() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let output = Command::new(env!("CARGO_BIN_EXE_deslicer"))
        .args(["--no-input", "--help"])
        .env_remove("DESLICER_NO_INPUT")
        .env_remove("CI")
        .output()
        .expect("spawn");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn agent_repl_under_no_input_exits_2_input_required() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = tempfile::tempdir().expect("home");
    let output = Command::new(env!("CARGO_BIN_EXE_deslicer"))
        .args([
            "--no-input",
            "--output",
            "json",
            "--ci-platform",
            "local",
            "agent",
            "repl",
        ])
        .env("HOME", home.path())
        .env_remove("DESLICER_API_TOKEN")
        .env_remove("DESLICER_DEV_TOKEN")
        .env_remove("OBSERVER_API_URL")
        .env_remove("CI")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("spawn");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(&stderr);
    let v: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| {
        panic!("last stderr not JSON ({e}): {line}\nfull:\n{stderr}");
    });
    assert_eq!(v["error"]["code"], "input.required");
    assert_eq!(v["error"]["exit_code"], 2);
    assert_eq!(v["error"]["kind"], "usage");
}
