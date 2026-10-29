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
    cmd.args(["auth", "whoami", "--log-format", "json"])
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
        .and(path("/api/v1/groups"))
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
async fn whoami_rejected_api_token_json_emits_single_document() {
    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/groups"))
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
        .expect("run auth whoami --log-format json");

    assert_eq!(
        output.status.code(),
        Some(4),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    let trimmed = combined.trim();

    // Exactly one top-level JSON value across both streams (pretty-printed ok).
    // A second document (e.g. emit_cli_error on stderr) makes from_str fail.
    let value: Value = serde_json::from_str(trimmed).unwrap_or_else(|err| {
        panic!("expected exactly one JSON value across stdout+stderr, got {err}: {trimmed}")
    });
    assert!(
        value.get("error").is_none(),
        "must not also emit cli_error envelope: {trimmed}"
    );
    assert_eq!(value["ok"], false);
    assert_eq!(value["logged_in"], false);
    assert_eq!(value["identity"], "none");
}

#[tokio::test]
async fn whoami_accepted_api_token_is_logged_in() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("DESLICER_API_TOKEN", "tools-key");

    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/groups"))
        .and(header("Authorization", "Bearer tools-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&observer)
        .await;

    let code = run(direct_ctx(&observer, LogFormat::Human), Args {}).await;
    assert_eq!(code, 0);

    std::env::remove_var("DESLICER_API_TOKEN");
}
