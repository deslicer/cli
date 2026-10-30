//! `auth login` non-TTY exit contract + static-token /me verify (issue #237).

#![allow(clippy::await_holding_lock)]

use deslicer_cli::cli::LogFormat;
use deslicer_cli::commands::auth::login::{run, Args};
use deslicer_cli::Ctx;
use serde_json::json;
use std::process::Command;
use std::sync::Mutex;
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

fn direct_ctx(observer: &MockServer, log_format: LogFormat) -> Ctx {
    Ctx {
        deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
        observer_api_url: Some(Url::parse(&format!("{}/", observer.uri())).expect("observer")),
        ci_override: None,
        log_format,
    }
}

#[test]
fn auth_login_non_tty_exits_4_with_token_hint() {
    let _guard = env_lock();
    let output = cli_bin()
        .args(["--output", "json", "auth", "login"])
        .env("CI", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run");
    assert_eq!(
        output.status.code(),
        Some(4),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let v = last_stderr_json(&String::from_utf8_lossy(&output.stderr));
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["error"]["code"], "auth.not_logged_in");
    assert_eq!(v["error"]["kind"], "auth");
    assert_eq!(v["error"]["exit_code"], 4);
    assert_ne!(
        v["error"]["code"], "auth.oidc",
        "must not mis-label TTY failure as auth.oidc"
    );
    let hint = v["error"]["hint"].as_str().unwrap_or("");
    assert!(
        hint.contains("DESLICER_API_TOKEN"),
        "hint should name DESLICER_API_TOKEN, got {hint:?}"
    );
    assert!(v["error"]["next_command"].is_null());
}

#[tokio::test]
async fn auth_login_rejected_api_token_does_not_claim_logged_in() {
    let _guard = env_lock();
    std::env::set_var("DESLICER_API_TOKEN", "not-a-real-token");

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .and(header("Authorization", "Bearer not-a-real-token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": "unauthorized",
            "message": "invalid token"
        })))
        .expect(1)
        .mount(&observer)
        .await;

    let code = run(
        direct_ctx(&observer, LogFormat::Human),
        Args { environment: None },
    )
    .await;
    assert_eq!(code, 4);

    std::env::remove_var("DESLICER_API_TOKEN");
}

#[tokio::test]
async fn auth_login_accepted_api_token_reports_logged_in() {
    let _guard = env_lock();
    std::env::set_var("DESLICER_API_TOKEN", "tools-key");

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .and(header("Authorization", "Bearer tools-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tenant_id": "2fb5ef22-12ad-4d20-9e0f-4736f47953bb"
        })))
        .expect(1)
        .mount(&observer)
        .await;

    let code = run(
        direct_ctx(&observer, LogFormat::Human),
        Args { environment: None },
    )
    .await;
    assert_eq!(code, 0);

    std::env::remove_var("DESLICER_API_TOKEN");
}

/// Observer without `GET /me` (HTML 404) must soft-succeed as logged in,
/// unverified — not exit 15 (issue #246).
#[tokio::test]
async fn auth_login_missing_me_endpoint_is_logged_in_unverified() {
    let _guard = env_lock();

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .and(header("Authorization", "Bearer tools-key"))
        .respond_with(
            ResponseTemplate::new(404)
                .insert_header("content-type", "text/html")
                .set_body_string("<html><body>404 Not Found</body></html>"),
        )
        .expect(1)
        .mount(&observer)
        .await;

    let output = cli_bin()
        .args(["--output", "json", "auth", "login"])
        .env("DESLICER_API_TOKEN", "tools-key")
        .env("OBSERVER_API_URL", format!("{}/", observer.uri()))
        .env("DESLICER_TOKEN_STORE", "memory")
        .env("CI", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run auth login");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "soft success must not emit an error envelope: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("login JSON on stdout");
    assert_eq!(doc["details"]["verified"], false);
    assert_eq!(doc["resolution_path"], "observer_api_token");
}
