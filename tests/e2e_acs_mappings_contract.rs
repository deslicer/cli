#![allow(clippy::await_holding_lock)]

#[path = "support/jwt_factory.rs"]
mod jwt_factory;

use deslicer_cli::ci::CiPlatform;
use deslicer_cli::cli::LogFormat;
use deslicer_cli::observer_client::{find_mapping, Client, MappingUpsert};
use deslicer_cli::resolver;
use deslicer_cli::token_source::TokenSource;
use deslicer_cli::Ctx;
use jwt_factory::mint_jwt;
use serde_json::json;
use url::Url;
use wiremock::matchers::{body_partial_json, method, path, query_param};
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

const BASE: &str = "/api/cli/observer/api/v1";

#[tokio::test]
async fn acs_stacks_list_reads_stack_credentials() {
    let deslicer = setup_proxy().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/acs/stacks/credentials")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"credentials": [
            {"id": "a", "stack_name": "zeta", "environment": "production", "created_at": "x", "updated_at": "y", "token_configured": true},
            {"id": "b", "stack_name": "acme", "environment": "staging", "created_at": "x", "updated_at": "y", "token_configured": true}
        ]})))
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, CiPlatform::Github).await;
    let stacks = client.list_acs_stacks().await.unwrap();
    let names: Vec<_> = stacks.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["acme", "zeta"]);
    assert!(stacks[0].token_configured);
}

#[tokio::test]
async fn acs_observe_posts_to_observe_route() {
    let deslicer = setup_proxy().await;
    Mock::given(method("POST"))
        .and(path(format!("{BASE}/acs/stacks/acme/observe")))
        .and(query_param("experience", "classic"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "stack_name": "acme",
            "object_count": 1,
            "objects": [{"resource_type": "index", "resource_id": "main", "content_hash": "h"}]
        })))
        .expect(1)
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, CiPlatform::Github).await;
    let report = client
        .observe_acs_stack("acme", Some("classic"))
        .await
        .unwrap();
    assert_eq!(report.stack, "acme");
    assert_eq!(report.object_count, 1);
    assert_eq!(report.objects[0].resource_id, "main");
}

#[tokio::test]
async fn acs_credential_test_reports_missing_credential_as_not_ok() {
    let deslicer = setup_proxy().await;
    Mock::given(method("POST"))
        .and(path(format!("{BASE}/acs/stacks/ghost/observe")))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "error": "acs_credentials_missing",
            "message": "No ACS credential stored for stack ghost"
        })))
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, CiPlatform::Github).await;
    let result = client.test_splunk_credential("ghost").await.unwrap();
    assert!(!result.ok);
    assert_eq!(result.stack, "ghost");
}

#[tokio::test]
async fn acs_credential_set_puts_token_without_echoing_it() {
    let deslicer = setup_proxy().await;
    Mock::given(method("PUT"))
        .and(path(format!("{BASE}/acs/stacks/acme/credentials")))
        .and(body_partial_json(
            json!({"token": "test-token-value", "environment": "staging"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "a", "stack_name": "acme", "environment": "staging",
            "created_at": "x", "updated_at": "y", "token_configured": true
        })))
        .expect(1)
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, CiPlatform::Github).await;
    let cred = client
        .set_acs_credential("acme", "test-token-value", Some("staging"))
        .await
        .unwrap();
    assert_eq!(cred.stack_name, "acme");
    let rendered = serde_json::to_string(&cred).unwrap();
    assert!(!rendered.contains("test-token-value"));
}

#[tokio::test]
async fn mappings_list_parses_app_name_rows() {
    let deslicer = setup_proxy().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/environments/prod/mappings")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "environment_name": "prod",
            "mappings": [{
                "id": "m1", "environment_name": "prod", "inventory_group": "indexers",
                "host_group_id": "019f36d6-3f61-7eea-9417-7ac4a8a10f69",
                "app_name": "ta_nix", "source_path": "apps/ta_nix", "dest_dir": null,
                "target_host_id": null, "target_host": null, "state": "present",
                "source": "db", "created_at": "x", "updated_at": "y"
            }]
        })))
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, CiPlatform::Github).await;
    let rows = client.list_environment_mappings("prod").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].app_name, "ta_nix");
    assert_eq!(rows[0].source.as_deref(), Some("db"));
}

#[tokio::test]
async fn mappings_upsert_sends_batch_body_without_replace() {
    let deslicer = setup_proxy().await;
    let group = "019f36d6-3f61-7eea-9417-7ac4a8a10f69";
    Mock::given(method("PUT"))
        .and(path(format!("{BASE}/environments/prod/mappings")))
        .and(body_partial_json(json!({
            "replace": false,
            "mappings": [{"host_group_id": group, "app_name": "ta_nix", "source_path": "apps/ta_nix"}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "environment_name": "prod",
            "mappings": [{"app_name": "ta_nix", "host_group_id": group, "inventory_group": "indexers", "source_path": "apps/ta_nix", "state": "present", "source": "db"}]
        })))
        .expect(1)
        .mount(&deslicer)
        .await;

    let client = proxy_client(&deslicer, CiPlatform::Github).await;
    let entry = MappingUpsert {
        host_group_id: group.into(),
        app_name: "ta_nix".into(),
        source_path: Some("apps/ta_nix".into()),
        dest_dir: None,
        target_host_id: None,
    };
    let rows = client
        .upsert_environment_mappings("prod", std::slice::from_ref(&entry))
        .await
        .unwrap();
    let row = find_mapping(&rows, &entry).expect("row");
    assert_eq!(row.inventory_group.as_deref(), Some("indexers"));
}
