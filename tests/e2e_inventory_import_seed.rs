#![allow(clippy::await_holding_lock)]

#[path = "support/jwt_factory.rs"]
mod jwt_factory;

use deslicer_cli::ci::CiPlatform;
use deslicer_cli::cli::LogFormat;
use deslicer_cli::observer_client::{AssignHostRequest, Client, CreateDeclaredHostRequest};
use deslicer_cli::resolver;
use deslicer_cli::token_source::TokenSource;
use deslicer_cli::Ctx;
use jwt_factory::mint_jwt;
use serde_json::json;
use url::Url;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

async fn proxy_client(deslicer: &MockServer, platform: CiPlatform) -> Client {
    let ctx = test_ctx(deslicer, platform);
    let jwt = mint_jwt(platform, json!({}));
    let backend = resolver::resolve(&ctx, &jwt, platform, None, None)
        .await
        .unwrap();
    Client::new(
        backend.observer_api_url,
        TokenSource::ci_oidc(platform, Some(jwt)),
    )
    .with_ci_platform(platform)
}

#[tokio::test]
async fn declared_host_and_assign_post_through_proxy() {
    let deslicer = setup_proxy().await;
    let row_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let host_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();

    Mock::given(method("POST"))
        .and(path("/api/cli/observer/api/v1/hosts/declared"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": row_id,
            "host_id": host_id,
            "hostname": "splunk-shd-1101",
            "origin": "declared"
        })))
        .mount(&deslicer)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/cli/observer/api/v1/inventory/assign"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "success": true,
            "message": "Host assigned to searchhead_deployer_shcluster_c1_ag1",
            "ansible_group_name": "searchhead_deployer_shcluster_c1_ag1"
        })))
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, CiPlatform::Github).await;
    let declared = client
        .create_declared_host(&CreateDeclaredHostRequest {
            hostname: "splunk-shd-1101",
            ansible_host: Some("2001:2042:2ee4:4e85::1000"),
        })
        .await
        .unwrap();
    assert_eq!(declared.id, row_id);
    assert_eq!(declared.host_id, host_id);

    let assigned = client
        .assign_inventory_host(&AssignHostRequest {
            host_id: declared.id,
            role: "searchhead_deployer".into(),
            availability_group: "ag1".into(),
            cluster_number: Some(1),
        })
        .await
        .unwrap();
    assert!(assigned.success);
    assert_eq!(
        assigned.ansible_group_name.as_deref(),
        Some("searchhead_deployer_shcluster_c1_ag1")
    );
}
