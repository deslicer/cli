#![allow(clippy::await_holding_lock)]

#[path = "support/jwt_factory.rs"]
mod jwt_factory;

use deslicer_cli::ci::CiPlatform;
use deslicer_cli::cli::LogFormat;
use deslicer_cli::observer_client::Client;
use deslicer_cli::oidc_exchange;
use deslicer_cli::resolver;
use deslicer_cli::token_source::TokenSource;
use deslicer_cli::Ctx;
use jwt_factory::mint_jwt;
use serde_json::json;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// External plan id (UUID v4 in production).
const PLAN_ID: &str = "0e4f8a34-1111-4222-8333-444455556666";
/// Internal change_plans row id (UUID v7 in production).
const PLAN_ROW_ID: &str = "01890a5d-7777-7888-9999-aaaabbbbcccc";
const EXECUTION_ID: &str = "01890a5d-eeee-7fff-8000-111122223333";

async fn setup() -> (MockServer, MockServer) {
    let deslicer = MockServer::start().await;
    let observer = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/cli/resolve-backend"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "observer_api_url": format!("{}/", observer.uri()),
            "audience": "https://api.deslicer.ai",
            "resolution_path": "tenant_default"
        })))
        .mount(&deslicer)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/auth/ci-oidc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "e2e-tools-token",
            "token_type": "Bearer",
            "expires_in": 900,
            "scope": "tools"
        })))
        .mount(&observer)
        .await;

    (deslicer, observer)
}

/// Proxy-mode setup: resolve-backend advertises the deslicer-ai CI proxy
/// base (`/api/cli/observer/`) and the CLI authenticates with the raw JWT.
async fn setup_proxy() -> MockServer {
    let deslicer = MockServer::start().await;
    let proxy_base = format!("{}/api/cli/observer/", deslicer.uri());

    Mock::given(method("POST"))
        .and(path("/api/cli/resolve-backend"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "observer_api_url": proxy_base,
            "audience": "https://api.deslicer.ai",
            "resolution_path": "tenant_default",
            "proxy_mode": true
        })))
        .mount(&deslicer)
        .await;

    deslicer
}

fn test_ctx(deslicer: &MockServer, platform: CiPlatform) -> Ctx {
    Ctx {
        deslicer_api_url: Url::parse(&deslicer.uri()).unwrap(),
        observer_api_url: None,
        ci_override: Some(platform),
        log_format: LogFormat::Human,
    }
}

async fn auth_client(deslicer: &MockServer, platform: CiPlatform) -> Client {
    let ctx = test_ctx(deslicer, platform);
    let jwt = mint_jwt(platform, json!({}));
    let backend = resolver::resolve(&ctx, &jwt, platform, None, None)
        .await
        .unwrap();
    let token = oidc_exchange::exchange(&backend.observer_api_url, &jwt, platform, None)
        .await
        .unwrap();
    Client::new(backend.observer_api_url, token)
}

async fn proxy_client(deslicer: &MockServer, platform: CiPlatform) -> Client {
    let ctx = test_ctx(deslicer, platform);
    let jwt = mint_jwt(platform, json!({}));
    let backend = resolver::resolve(&ctx, &jwt, platform, None, None)
        .await
        .unwrap();
    assert!(backend.proxy_mode);
    Client::new(
        backend.observer_api_url,
        TokenSource::ci_oidc(platform, Some(jwt)),
    )
    .with_ci_platform(platform)
}

async fn test_auth_login(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    let ctx = test_ctx(&deslicer, platform);
    let jwt = mint_jwt(platform, json!({}));

    let backend = resolver::resolve(&ctx, &jwt, platform, None, None)
        .await
        .unwrap();
    assert_eq!(
        backend.observer_api_url.as_str(),
        format!("{}/", observer.uri())
    );
    assert_eq!(backend.resolution_path, "tenant_default");

    let token = oidc_exchange::exchange(&backend.observer_api_url, &jwt, platform, None)
        .await
        .unwrap();
    assert!(!token.is_empty());
}

/// `change plan` — proxy orchestration: create draft + trigger compile.
async fn test_change_plan(platform: CiPlatform) {
    let deslicer = setup_proxy().await;
    Mock::given(method("POST"))
        .and(path("/api/cli/observer/v1/plan"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plan_id": PLAN_ID,
            "plan_row_id": PLAN_ROW_ID,
            "status": "draft",
            "compile": { "accepted": true }
        })))
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, platform).await;
    let created = client.create_plan_orchestrated(None).await.unwrap();
    assert_eq!(created.plan_id, PLAN_ID);
    assert_eq!(created.plan_row_id.as_deref(), Some(PLAN_ROW_ID));
    assert_eq!(created.status, "draft");
}

/// `change verify` — proxy orchestration: dry-run compile.
async fn test_change_verify(platform: CiPlatform) {
    let deslicer = setup_proxy().await;
    Mock::given(method("POST"))
        .and(path("/api/cli/observer/v1/plan/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plan_id": PLAN_ROW_ID,
            "accepted": true,
            "dry_run": true
        })))
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, platform).await;
    client
        .verify_plan_orchestrated(PLAN_ROW_ID, Some("main"))
        .await
        .unwrap();
}

async fn test_change_show(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": PLAN_ROW_ID,
            "plan_id": PLAN_ID,
            "tenant_id": "11111111-2222-4333-8444-555566667777",
            "status": "pending_approval",
            "name": "ci: main@abc1234",
            "source_type": "git"
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let plan = client.get_plan(PLAN_ID).await.unwrap();
    assert_eq!(plan.id, PLAN_ROW_ID);
    assert_eq!(plan.external_id(), PLAN_ID);
    assert_eq!(plan.status, "pending_approval");
}

async fn test_change_approve(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/approve")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": PLAN_ROW_ID,
            "plan_id": PLAN_ID,
            "status": "approved"
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let plan = client.approve(PLAN_ID).await.unwrap();
    assert_eq!(plan.status, "approved");
}

async fn test_change_reject(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/reject")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": PLAN_ROW_ID,
            "plan_id": PLAN_ID,
            "status": "rejected"
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let plan = client.reject(PLAN_ID, "e2e rejection reason").await.unwrap();
    assert_eq!(plan.status, "rejected");
}

/// `change deploy` — execute returns an ExecutePlanResponse, not a plan.
async fn test_change_deploy(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/execute")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "execution_id": EXECUTION_ID,
            "plan_id": PLAN_ID,
            "status": "queued",
            "jobs_total": 3
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let queued = client.execute(PLAN_ID).await.unwrap();
    assert_eq!(queued.execution_id, EXECUTION_ID);
    assert_eq!(queued.status, "queued");
    assert_eq!(queued.jobs_total, 3);
}

/// Execution monitoring — GET /api/v1/executions/{id}.
async fn test_execution_summary(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/executions/{EXECUTION_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "execution_id": EXECUTION_ID,
            "plan_id": PLAN_ROW_ID,
            "external_plan_id": PLAN_ID,
            "status": "succeeded",
            "rollout_strategy": "rolling",
            "jobs_total": 3,
            "jobs_succeeded": 3,
            "jobs_failed": 0,
            "jobs": []
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let summary = client.get_execution(EXECUTION_ID).await.unwrap();
    assert!(summary.is_terminal());
    assert!(summary.is_success());
    assert_eq!(summary.jobs_succeeded, 3);
}

/// `change status` — real PlanProgress shape.
async fn test_change_status(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/progress")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plan_id": PLAN_ID,
            "progress_status": "completed",
            "total_items": 5,
            "fully_completed_items": 5,
            "total_host_items": 15
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let progress = client.progress(PLAN_ID).await.unwrap();
    assert_eq!(progress.progress_status, "completed");
    assert!(progress.is_terminal());
    assert_eq!(progress.fully_completed_items, 5);
}

#[tokio::test]
async fn github_auth_login() {
    test_auth_login(CiPlatform::Github).await;
}

#[tokio::test]
async fn github_change_plan() {
    test_change_plan(CiPlatform::Github).await;
}

#[tokio::test]
async fn github_change_show() {
    test_change_show(CiPlatform::Github).await;
}

#[tokio::test]
async fn github_change_approve() {
    test_change_approve(CiPlatform::Github).await;
}

#[tokio::test]
async fn github_change_reject() {
    test_change_reject(CiPlatform::Github).await;
}

#[tokio::test]
async fn github_change_deploy() {
    test_change_deploy(CiPlatform::Github).await;
}

#[tokio::test]
async fn github_execution_summary() {
    test_execution_summary(CiPlatform::Github).await;
}

#[tokio::test]
async fn github_change_verify() {
    test_change_verify(CiPlatform::Github).await;
}

#[tokio::test]
async fn github_change_status() {
    test_change_status(CiPlatform::Github).await;
}

#[tokio::test]
async fn gitlab_auth_login() {
    test_auth_login(CiPlatform::Gitlab).await;
}

#[tokio::test]
async fn gitlab_change_plan() {
    test_change_plan(CiPlatform::Gitlab).await;
}

#[tokio::test]
async fn gitlab_change_show() {
    test_change_show(CiPlatform::Gitlab).await;
}

#[tokio::test]
async fn gitlab_change_approve() {
    test_change_approve(CiPlatform::Gitlab).await;
}

#[tokio::test]
async fn gitlab_change_reject() {
    test_change_reject(CiPlatform::Gitlab).await;
}

#[tokio::test]
async fn gitlab_change_deploy() {
    test_change_deploy(CiPlatform::Gitlab).await;
}

#[tokio::test]
async fn gitlab_change_verify() {
    test_change_verify(CiPlatform::Gitlab).await;
}

#[tokio::test]
async fn gitlab_change_status() {
    test_change_status(CiPlatform::Gitlab).await;
}

#[tokio::test]
async fn azure_auth_login() {
    test_auth_login(CiPlatform::Azure).await;
}

#[tokio::test]
async fn azure_change_plan() {
    test_change_plan(CiPlatform::Azure).await;
}

#[tokio::test]
async fn azure_change_show() {
    test_change_show(CiPlatform::Azure).await;
}

#[tokio::test]
async fn azure_change_approve() {
    test_change_approve(CiPlatform::Azure).await;
}

#[tokio::test]
async fn azure_change_reject() {
    test_change_reject(CiPlatform::Azure).await;
}

#[tokio::test]
async fn azure_change_deploy() {
    test_change_deploy(CiPlatform::Azure).await;
}

#[tokio::test]
async fn azure_change_verify() {
    test_change_verify(CiPlatform::Azure).await;
}

#[tokio::test]
async fn azure_change_status() {
    test_change_status(CiPlatform::Azure).await;
}

#[tokio::test]
async fn bitbucket_auth_login() {
    test_auth_login(CiPlatform::Bitbucket).await;
}

#[tokio::test]
async fn bitbucket_change_plan() {
    test_change_plan(CiPlatform::Bitbucket).await;
}

#[tokio::test]
async fn bitbucket_change_show() {
    test_change_show(CiPlatform::Bitbucket).await;
}

#[tokio::test]
async fn bitbucket_change_approve() {
    test_change_approve(CiPlatform::Bitbucket).await;
}

#[tokio::test]
async fn bitbucket_change_reject() {
    test_change_reject(CiPlatform::Bitbucket).await;
}

#[tokio::test]
async fn bitbucket_change_deploy() {
    test_change_deploy(CiPlatform::Bitbucket).await;
}

#[tokio::test]
async fn bitbucket_change_verify() {
    test_change_verify(CiPlatform::Bitbucket).await;
}

#[tokio::test]
async fn bitbucket_change_status() {
    test_change_status(CiPlatform::Bitbucket).await;
}
