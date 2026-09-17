// ENV_LOCK only serializes env access across single-threaded tests;
// holding it across the await is safe (no cross-task contention).
#![allow(clippy::await_holding_lock)]

use deslicer_cli::cli::LogFormat; // pragma: allowlist secret
use deslicer_cli::commands::change::status::{run, Args}; // pragma: allowlist secret
use deslicer_cli::Ctx; // pragma: allowlist secret
use serde_json::json;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static ENV_LOCK: Mutex<()> = Mutex::new(());

const PLAN_ID: &str = "0e4f8a34-1111-4222-8333-444455556666";
const PLAN_ROW_ID: &str = "01890a5d-7777-7888-9999-aaaabbbbcccc";

fn direct_ctx(observer: &MockServer) -> Ctx {
    Ctx {
        deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"), // pragma: allowlist secret
        observer_api_url: Some(Url::parse(&observer.uri()).expect("observer")),
        ci_override: None,
        log_format: LogFormat::Human,
    }
}

async fn mount_plan(observer: &MockServer, status: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}")))
        .and(header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": PLAN_ROW_ID,
            "plan_id": PLAN_ID,
            "status": status,
            "name": "pending plan"
        })))
        .mount(observer)
        .await;
}

#[tokio::test]
async fn change_status_not_started_returns_immediately() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("DESLICER_API_TOKEN", "test-token"); // pragma: allowlist secret

    let observer = MockServer::start().await;
    mount_plan(&observer, "pending_approval").await;

    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ROW_ID}/diff")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&observer)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/progress")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plan_id": PLAN_ID,
            "progress_status": "not_started",
            "total_items": 0,
            "fully_completed_items": 0
        })))
        .expect(1)
        .mount(&observer)
        .await;

    let started = Instant::now();
    let code = run(
        direct_ctx(&observer),
        Args {
            plan_id: PLAN_ID.to_string(),
        },
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(code, 0);

    std::env::remove_var("DESLICER_API_TOKEN"); // pragma: allowlist secret
}

#[tokio::test]
async fn change_status_partial_polls_until_terminal() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("DESLICER_API_TOKEN", "test-token"); // pragma: allowlist secret

    let observer = MockServer::start().await;
    mount_plan(&observer, "executing").await;

    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ROW_ID}/diff")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&observer)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/progress")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plan_id": PLAN_ID,
            "progress_status": "partial",
            "total_items": 4,
            "fully_completed_items": 2
        })))
        .up_to_n_times(1)
        .mount(&observer)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/progress")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plan_id": PLAN_ID,
            "progress_status": "completed",
            "total_items": 4,
            "fully_completed_items": 4
        })))
        .mount(&observer)
        .await;

    let code = run(
        direct_ctx(&observer),
        Args {
            plan_id: PLAN_ID.to_string(),
        },
    )
    .await;
    assert_eq!(code, 0);

    std::env::remove_var("DESLICER_API_TOKEN"); // pragma: allowlist secret
}
