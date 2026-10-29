//! Bare command groups with no subcommand must exit 2 (usage), not 0.
//! Refs #187 — clap prints help, but the process status is a usage failure.

use std::process::Command;

fn deslicer() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer"));
    cmd.env_remove("DESLICER_OUTPUT")
        .env_remove("CI")
        .env_remove("GITHUB_ACTIONS");
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
fn bare_groups_exit_2() {
    for args in [
        &["change"][..],
        &["acs", "stacks"][..],
        &["models", "byok"][..],
        &["org", "settings"][..],
        &[][..],
    ] {
        let out = deslicer().args(args).output().expect("run deslicer");
        assert_eq!(
            out.status.code(),
            Some(2),
            "args={args:?}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        // Non-TTY stdout defaults to JSON (#231): usage envelope on stderr.
        assert!(
            !out.stderr.is_empty(),
            "expected usage envelope on stderr for {args:?}"
        );
    }
}

#[test]
fn bare_group_json_still_usage_envelope_exit_2() {
    let out = deslicer()
        .args(["--output", "json", "change"])
        .output()
        .expect("run deslicer");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    let v = last_stderr_json(&err);
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["error"]["kind"], "usage");
    assert_eq!(v["error"]["code"], "usage.invalid");
    assert_eq!(v["error"]["exit_code"], 2);
    assert_eq!(v["error"]["retryable"], false);
    assert!(v["error"]["next_command"].is_null());
    assert!(v["error"]["details"].is_object());
}

#[test]
fn explicit_help_still_exit_0() {
    let out = deslicer()
        .args(["change", "--help"])
        .output()
        .expect("run deslicer");
    assert_eq!(out.status.code(), Some(0));
}
