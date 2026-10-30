//! Global `--no-input` / `--non-interactive` / `DESLICER_NO_INPUT` (issue #227).

#![allow(clippy::await_holding_lock)]

use clap::CommandFactory;
use deslicer_cli::cli::Cli;
use serde_json::json;
use std::process::Command;
use std::sync::Mutex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn parse_json_output(text: &str) -> serde_json::Value {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return value;
    }
    let line = trimmed
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(trimmed);
    serde_json::from_str(line)
        .unwrap_or_else(|e| panic!("output is not JSON ({e}): {line:?}\nfull:\n{text}"))
}

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

/// Agents invoke exactly `--no-input auth login --no-wait`; that must reach the
/// device start path (not the fail-fast `auth.not_logged_in` gate).
#[tokio::test]
async fn no_input_auth_login_no_wait_starts_device_flow() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/cli/device/start"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "devcode1234567890abcd",
            "user_code": "ABCD-EFGH",
            "verification_uri": "https://app.deslicer.ai/dashboard/cli-auth",
            "verification_uri_complete": "https://app.deslicer.ai/dashboard/cli-auth?user_code=ABCD-EFGH",
            "interval": 1,
            "expires_in": 60
        })))
        .expect(1)
        .mount(&server)
        .await;

    let home = tempfile::tempdir().expect("home");
    let api_url = format!("{}/", server.uri());
    let output = Command::new(env!("CARGO_BIN_EXE_deslicer"))
        .args([
            "--no-input",
            "--output",
            "json",
            "--deslicer-api-url",
            &api_url,
            "--ci-platform",
            "local",
            "auth",
            "login",
            "--no-wait",
        ])
        .env("HOME", home.path())
        .env("DESLICER_CONFIG_DIR", home.path().join("config"))
        .env("DESLICER_TOKEN_STORE", "file")
        .env_remove("DESLICER_API_TOKEN")
        .env_remove("DESLICER_DEV_TOKEN")
        .env_remove("OBSERVER_API_URL")
        .env_remove("DESLICER_NO_INPUT")
        .env_remove("CI")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("spawn");

    assert_eq!(
        output.status.code(),
        Some(4),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let pending = parse_json_output(&stdout);
    assert_eq!(pending["user_code"], "ABCD-EFGH");
    assert_eq!(pending["pending"], true);
    assert_eq!(pending["logged_in"], false);

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err = parse_json_output(&stderr);
    assert_eq!(err["error"]["code"], "auth.pending");
    assert_eq!(err["error"]["exit_code"], 4);
    assert_eq!(err["error"]["retryable"], true);
    assert_ne!(err["error"]["code"], "auth.not_logged_in");
}

/// Agents that pass `--no-input auth login --wait` must see the device code /
/// URL before the CLI blocks on poll (otherwise approval is impossible).
#[tokio::test]
async fn no_input_auth_login_wait_emits_device_code_before_poll() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/cli/device/start"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "devcode1234567890abcd",
            "user_code": "WXYZ-1234",
            "verification_uri": "https://app.deslicer.ai/dashboard/cli-auth",
            "verification_uri_complete": "https://app.deslicer.ai/dashboard/cli-auth?user_code=WXYZ-1234",
            "interval": 1,
            "expires_in": 60
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/cli/device/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "cli_session_token": format!("dslcli_{}", "ef".repeat(32)),
            "expires_at": "2099-01-01T00:00:00.000Z",
            "tenant_id": "tenant-wait",
            "display_name": "Wait Agent"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let home = tempfile::tempdir().expect("home");
    let api_url = format!("{}/", server.uri());
    let output = Command::new(env!("CARGO_BIN_EXE_deslicer"))
        .args([
            "--no-input",
            "--output",
            "json",
            "--deslicer-api-url",
            &api_url,
            "--ci-platform",
            "local",
            "auth",
            "login",
            "--wait",
        ])
        .env("HOME", home.path())
        .env("DESLICER_CONFIG_DIR", home.path().join("config"))
        .env("DESLICER_TOKEN_STORE", "file")
        .env_remove("DESLICER_API_TOKEN")
        .env_remove("DESLICER_DEV_TOKEN")
        .env_remove("OBSERVER_API_URL")
        .env_remove("DESLICER_NO_INPUT")
        .env_remove("CI")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("spawn");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("WXYZ-1234"),
        "agent must see user_code before/with login success:\n{stdout}"
    );
    assert!(
        stdout.contains("cli-auth") || stdout.contains("verification_uri"),
        "agent must see verification URI:\n{stdout}"
    );

    // Success JSON is pretty-printed; stream-parse consecutive documents.
    let docs: Vec<serde_json::Value> = serde_json::Deserializer::from_str(&stdout)
        .into_iter::<serde_json::Value>()
        .filter_map(Result::ok)
        .collect();
    assert!(
        docs.len() >= 2,
        "expected pending JSON then login JSON, got {} doc(s):\n{stdout}",
        docs.len()
    );
    assert_eq!(docs[0]["user_code"], "WXYZ-1234");
    assert_eq!(docs[0]["pending"], true);
    assert_eq!(docs[0]["logged_in"], false);
    assert_eq!(docs.last().unwrap()["resolution_path"], "device_session");
}
