// ENV_LOCK only serializes env access across single-threaded tests;
// holding it across the await is safe (no cross-task contention).
#![allow(clippy::await_holding_lock)]

use deslicer_cli::cli::LogFormat;
use deslicer_cli::commands::auth::whoami::{run, Args};
use deslicer_cli::Ctx;
use serde_json::json;
use std::sync::Mutex;
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn direct_ctx(observer: &MockServer) -> Ctx {
    Ctx {
        deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
        observer_api_url: Some(Url::parse(&format!("{}/", observer.uri())).expect("observer")),
        ci_override: None,
        log_format: LogFormat::Human,
    }
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

    let code = run(direct_ctx(&observer), Args {}).await;
    assert_eq!(code, 4);

    std::env::remove_var("DESLICER_API_TOKEN");
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

    let code = run(direct_ctx(&observer), Args {}).await;
    assert_eq!(code, 0);

    std::env::remove_var("DESLICER_API_TOKEN");
}
