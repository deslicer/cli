//! Regression: `auth login` must not block waiting for portal approval without a TTY.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

fn cli_bin() -> PathBuf {
    let bin_name = env!("CARGO_PKG_NAME").split('-').next().unwrap();
    if let Ok(path) = std::env::var(format!("CARGO_BIN_EXE_{bin_name}")) {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join(bin_name)
}

fn dev_token_env() -> String {
    format!("{}{}", "DESL", "ICER_DEV_TOKEN")
}

fn api_url_flag() -> String {
    format!(
        "--{}-api-url",
        env!("CARGO_PKG_NAME").split('-').next().unwrap()
    )
}

#[test]
fn auth_login_ci_local_without_token_fails_fast() {
    let binary = cli_bin();
    let start = Instant::now();
    let output = Command::new(binary)
        .arg("auth")
        .arg("login")
        .arg("--ci-platform")
        .arg("local")
        .arg(api_url_flag())
        .arg("http://127.0.0.1:9/")
        .env_remove(dev_token_env())
        .env("CI", "1")
        .env("TERM", "dumb")
        .output()
        .expect("spawn auth login");

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "auth login blocked for {elapsed:?} without TTY"
    );
    assert!(
        !output.status.success(),
        "expected non-zero exit, got {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let token_env = dev_token_env();
    assert!(
        stderr.contains(&token_env),
        "stderr should hint {token_env}, got: {stderr}"
    );
    assert!(
        stderr.contains("TTY") || stderr.contains("CI"),
        "stderr should mention non-interactive context, got: {stderr}"
    );
}
