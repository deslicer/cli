#![allow(clippy::await_holding_lock)]

#[path = "support/jwt_factory.rs"]
mod jwt_factory;

use deslicer_cli::ci::CiPlatform;
use deslicer_cli::cli::LogFormat;
use deslicer_cli::observer_client::{Client, ReconcileMode};
use deslicer_cli::oidc_exchange;
use deslicer_cli::resolver;
use deslicer_cli::Ctx;
use jwt_factory::mint_jwt;
use serde_json::json;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PLAN_ID: &str = "plan-e2e-001";

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

async fn test_change_plan(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/state/reconcile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": PLAN_ID,
            "status": "draft",
            "summary": "plan created"
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let plan = client
        .reconcile(&None, ReconcileMode::PlanOnly)
        .await
        .unwrap();
    assert_eq!(plan.id, PLAN_ID);
    assert_eq!(plan.status, "draft");
}

async fn test_change_show(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": PLAN_ID,
            "status": "draft",
            "summary": "show plan"
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let plan = client.get_plan(PLAN_ID).await.unwrap();
    assert_eq!(plan.id, PLAN_ID);
}

async fn test_change_approve(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/approve")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": PLAN_ID,
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
            "id": PLAN_ID,
            "status": "rejected"
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let plan = client.reject(PLAN_ID).await.unwrap();
    assert_eq!(plan.status, "rejected");
}

async fn test_change_deploy(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/execute")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": PLAN_ID,
            "status": "executing"
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let plan = client.execute(PLAN_ID).await.unwrap();
    assert_eq!(plan.status, "executing");
}

async fn test_change_verify(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/state/reconcile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": PLAN_ID,
            "status": "clean"
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let plan = client
        .reconcile(&None, ReconcileMode::PlanOnly)
        .await
        .unwrap();
    assert_eq!(plan.status, "clean");
}

async fn test_change_status(platform: CiPlatform) {
    let (deslicer, observer) = setup().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/progress")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": PLAN_ID,
            "status": "succeeded"
        })))
        .mount(&observer)
        .await;

    let client = auth_client(&deslicer, platform).await;
    let progress = client.progress(PLAN_ID).await.unwrap();
    assert_eq!(progress.status, "succeeded");
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
