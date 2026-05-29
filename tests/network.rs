// ENV_LOCK only serializes env access across single-threaded tests;
// holding it across the await is safe (no cross-task contention).
#![allow(clippy::await_holding_lock)]

use deslicer_cli::ci::CiPlatform;
use deslicer_cli::errors::CliError;
use deslicer_cli::observer_client::{Client, ReconcileMode};
use deslicer_cli::oidc_exchange;
use deslicer_cli::resolver;
use deslicer_cli::Ctx;
use std::sync::Mutex;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn test_ctx(deslicer_api_url: url::Url, observer_api_url: Option<url::Url>) -> Ctx {
    Ctx {
        deslicer_api_url,
        observer_api_url,
        ci_override: None,
        log_format: deslicer_cli::cli::LogFormat::Human,
    }
}

#[tokio::test]
async fn resolve_happy_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/cli/resolve-backend"))
        .and(header("Authorization", "Bearer test-jwt"))
        .and(header("X-Deslicer-CI-Platform", "github"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "observer_api_url": "https://observer.example.test/",
            "audience": "https://api.deslicer.ai",
            "resolution_path": "installation_binding"
        })))
        .mount(&server)
        .await;

    let ctx = test_ctx(url::Url::parse(&server.uri()).unwrap(), None);
    let backend = resolver::resolve(&ctx, "test-jwt", CiPlatform::Github, Some("staging"), None)
        .await
        .unwrap();

    assert_eq!(
        backend.observer_api_url.as_str(),
        "https://observer.example.test/"
    );
    assert_eq!(backend.audience, "https://api.deslicer.ai");
    assert_eq!(backend.resolution_path, "installation_binding");
}

#[tokio::test]
async fn resolve_short_circuit_without_http() {
    let server = MockServer::start().await;
    let override_url = url::Url::parse("https://observer.override.test/").unwrap();
    let ctx = test_ctx(
        url::Url::parse(&server.uri()).unwrap(),
        Some(override_url.clone()),
    );

    let backend = resolver::resolve(&ctx, "ignored", CiPlatform::Github, None, None)
        .await
        .unwrap();

    assert_eq!(backend.observer_api_url, override_url);
    assert_eq!(backend.resolution_path, "observer_url_override");
    assert_eq!(backend.audience, deslicer_cli::ci::AUDIENCE);
}

#[tokio::test]
async fn resolve_maps_401_to_exit_4() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/cli/resolve-backend"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "detail": "invalid token"
        })))
        .mount(&server)
        .await;

    let ctx = test_ctx(url::Url::parse(&server.uri()).unwrap(), None);
    let err = resolver::resolve(&ctx, "bad", CiPlatform::Github, None, None)
        .await
        .unwrap_err();

    assert!(matches!(err, CliError::OidcRejected(_)));
    assert_eq!(err.exit_code(), 4);
}

#[tokio::test]
async fn resolve_maps_403_to_exit_5() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/cli/resolve-backend"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "detail": "repo not allowlisted"
        })))
        .mount(&server)
        .await;

    let ctx = test_ctx(url::Url::parse(&server.uri()).unwrap(), None);
    let err = resolver::resolve(&ctx, "jwt", CiPlatform::Github, None, None)
        .await
        .unwrap_err();

    assert!(matches!(err, CliError::RepoNotAllowlisted(_)));
    assert_eq!(err.exit_code(), 5);
}

#[tokio::test]
async fn resolve_maps_429_to_exit_9() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/cli/resolve-backend"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "42")
                .set_body_json(serde_json::json!({ "detail": "slow down" })),
        )
        .mount(&server)
        .await;

    let ctx = test_ctx(url::Url::parse(&server.uri()).unwrap(), None);
    let err = resolver::resolve(&ctx, "jwt", CiPlatform::Github, None, None)
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        CliError::RateLimited {
            retry_after_secs: 42
        }
    ));
    assert_eq!(err.exit_code(), 9);
}

#[tokio::test]
async fn exchange_happy_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/ci-oidc"))
        .and(header("Authorization", "Bearer ci-jwt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "dap_tools_key",
            "token_type": "Bearer",
            "expires_in": 900,
            "scope": "tools"
        })))
        .mount(&server)
        .await;

    let base = url::Url::parse(&format!("{}/", server.uri())).unwrap();
    let token = oidc_exchange::exchange(&base, "ci-jwt", CiPlatform::Github, Some("prod"))
        .await
        .unwrap();

    assert_eq!(token, "dap_tools_key");
}

#[tokio::test]
async fn exchange_maps_401_to_exit_4() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/ci-oidc"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "detail": "rejected"
        })))
        .mount(&server)
        .await;

    let base = url::Url::parse(&format!("{}/", server.uri())).unwrap();
    let err = oidc_exchange::exchange(&base, "bad", CiPlatform::Github, None)
        .await
        .unwrap_err();

    assert!(matches!(err, CliError::OidcRejected(_)));
    assert_eq!(err.exit_code(), 4);
}

#[tokio::test]
async fn client_reconcile_happy_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/state/reconcile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "plan-1",
            "status": "draft",
            "summary": "reconcile ok"
        })))
        .mount(&server)
        .await;

    let base = url::Url::parse(&format!("{}/", server.uri())).unwrap();
    let client = Client::new(base, "tools-token".to_string());
    let plan = client
        .reconcile(&Some("staging".to_string()), ReconcileMode::PlanOnly)
        .await
        .unwrap();

    assert_eq!(plan.id, "plan-1");
    assert_eq!(plan.status, "draft");
    assert_eq!(plan.summary.as_deref(), Some("reconcile ok"));
}

#[tokio::test]
async fn client_retries_503_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/plans/plan-retry"))
        .respond_with(
            ResponseTemplate::new(503).set_body_json(serde_json::json!({ "detail": "busy" })),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/plans/plan-retry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "plan-retry",
            "status": "approved"
        })))
        .mount(&server)
        .await;

    let base = url::Url::parse(&format!("{}/", server.uri())).unwrap();
    let client = Client::new(base, "tools-token".to_string());
    let plan = client.get_plan("plan-retry").await.unwrap();

    assert_eq!(plan.id, "plan-retry");
    assert_eq!(plan.status, "approved");
}

#[tokio::test]
async fn client_get_plan_404_maps_to_exit_11() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/plans/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "detail": "not found"
        })))
        .mount(&server)
        .await;

    let base = url::Url::parse(&format!("{}/", server.uri())).unwrap();
    let client = Client::new(base, "tools-token".to_string());
    let err = client.get_plan("missing").await.unwrap_err();

    assert!(matches!(err, CliError::PlanNotFound(_)));
    assert_eq!(err.exit_code(), 11);
}

#[test]
fn repo_from_github_env_when_set() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("GITHUB_REPOSITORY", "org/repo");
    let server = tokio::runtime::Runtime::new().unwrap();
    server.block_on(async {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/cli/resolve-backend"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "observer_api_url": "https://observer.example.test/",
                "audience": "https://api.deslicer.ai",
                "resolution_path": "binding"
            })))
            .mount(&mock)
            .await;

        let ctx = test_ctx(url::Url::parse(&mock.uri()).unwrap(), None);
        resolver::resolve(&ctx, "jwt", CiPlatform::Github, None, None)
            .await
            .unwrap();
    });
    std::env::remove_var("GITHUB_REPOSITORY");
}
