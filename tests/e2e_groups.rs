#![allow(clippy::await_holding_lock)]

#[path = "support/jwt_factory.rs"]
mod jwt_factory;

use deslicer_cli::ci::CiPlatform;
use deslicer_cli::cli::LogFormat;
use deslicer_cli::observer_client::Client;
use deslicer_cli::resolver;
use deslicer_cli::token_source::TokenSource;
use deslicer_cli::Ctx;
use jwt_factory::mint_jwt;
use serde_json::json;
use url::Url;
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
async fn groups_list_reads_host_groups_through_proxy() {
    let deslicer = setup_proxy().await;
    Mock::given(method("GET"))
        .and(path("/api/cli/observer/api/v1/groups"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": "019f36d6-3f61-7eea-9417-7ac4a8a10f69",
            "name": "search-heads",
            "display_name": "Search Heads",
            "member_count": 2
        }])))
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, CiPlatform::Github).await;
    let groups = client.list_groups().await.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].id, "019f36d6-3f61-7eea-9417-7ac4a8a10f69");
    assert_eq!(groups[0].name, "search-heads");
    assert_eq!(groups[0].member_count, Some(2));
}
