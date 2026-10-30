// ENV_LOCK only serializes env access across single-threaded tests;
// holding it across the await is safe (no cross-task contention).
#![allow(clippy::await_holding_lock)]

use deslicer_cli::cli::LogFormat;
use deslicer_cli::commands::auth::whoami::{run, Args};
use deslicer_cli::Ctx;
use serde_json::{json, Value};
use std::process::Command;
use std::sync::Mutex;
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn direct_ctx(observer: &MockServer, log_format: LogFormat) -> Ctx {
    Ctx {
        deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
        observer_api_url: Some(Url::parse(&format!("{}/", observer.uri())).expect("observer")),
        ci_override: None,
        log_format,
    }
}

fn whoami_json_cmd(observer: &MockServer) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer"));
    cmd.args(["--output", "json", "auth", "whoami"])
        .env("OBSERVER_API_URL", format!("{}/", observer.uri()))
        .env("DESLICER_API_TOKEN", "not-a-real-token")
        .env("DESLICER_TOKEN_STORE", "memory");
    for key in [
        "DESLICER_DEV_TOKEN",
        "DESLICER_CONFIG_DIR",
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "TF_BUILD",
        "BITBUCKET_BUILD_NUMBER",
        "ACTIONS_ID_TOKEN_REQUEST_URL",
        "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
        "DESLICER_DOTENV_PATH",
    ] {
        cmd.env_remove(key);
    }
    cmd
}

#[tokio::test]
async fn whoami_rejected_api_token_is_not_logged_in() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("DESLICER_API_TOKEN", "not-a-real-token");

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .and(header("Authorization", "Bearer not-a-real-token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": "oidc_rejected",
            "message": "oidc_rejected"
        })))
        .expect(1)
        .mount(&observer)
        .await;

    let code = run(direct_ctx(&observer, LogFormat::Human), Args {}).await;
    assert_eq!(code, 4);

    std::env::remove_var("DESLICER_API_TOKEN");
}

#[tokio::test]
async fn whoami_rejected_api_token_json_emits_envelope_on_stderr() {
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

    let output = whoami_json_cmd(&observer)
        .output()
        .expect("run auth whoami --output json");

    assert_eq!(
        output.status.code(),
        Some(4),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "rejected-token whoami must not print a non-envelope document on stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(&stderr);
    let value: Value = serde_json::from_str(line).unwrap_or_else(|err| {
        panic!("last stderr line is not JSON ({err}): {line:?}\nfull stderr:\n{stderr}")
    });
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["error"]["code"], "auth.api_token_rejected");
    assert_eq!(value["error"]["exit_code"], 4);
    assert_eq!(value["error"]["next_command"], "deslicer auth login");
    assert!(value["error"]["hint"].is_string());
}

#[tokio::test]
async fn whoami_accepted_api_token_is_logged_in() {
    let _guard = ENV_LOCK.lock().unwrap();
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

    let code = run(direct_ctx(&observer, LogFormat::Human), Args {}).await;
    assert_eq!(code, 0);

    std::env::remove_var("DESLICER_API_TOKEN");
}

/// 403 on `GET api/v1/me` means Observer accepted the bearer then forbade the
/// call (e.g. narrow scope). That is not 401 auth rejection: whoami must still
/// report logged in / exit 0, not the not-logged-in exit 4 path.
#[tokio::test]
async fn whoami_forbidden_scoped_api_token_is_still_logged_in() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("DESLICER_API_TOKEN", "narrow-scope-key");

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .and(header("Authorization", "Bearer narrow-scope-key"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": "forbidden",
            "message": "missing scope: read"
        })))
        .expect(1)
        .mount(&observer)
        .await;

    let code = run(direct_ctx(&observer, LogFormat::Human), Args {}).await;
    assert_eq!(
        code, 0,
        "403 after auth must not be reported as not-logged-in (exit 4)"
    );

    std::env::remove_var("DESLICER_API_TOKEN");
}
