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
async fn inventory_list_reads_ansible_groups_through_proxy() {
    let deslicer = setup_proxy().await;
    Mock::given(method("GET"))
        .and(path("/api/cli/observer/api/v1/inventory"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "_meta": {"hostvars": {}},
            "all": {"children": ["forwarders"]},
            "forwarders": {"hosts": ["idx1"]}
        })))
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, CiPlatform::Github).await;
    let groups = client.list_inventory().await.unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].name, "all");
    assert_eq!(groups[1].name, "forwarders");
    assert_eq!(groups[1].hosts, vec!["idx1"]);
}
