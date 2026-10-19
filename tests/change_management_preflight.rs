use deslicer_cli::observer_client::Client; // pragma: allowlist secret
use deslicer_cli::token_source::TokenSource; // pragma: allowlist secret
use serde_json::json;
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn bundle_preflight_rejects_git_backed_tenant_before_upload() {
    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/admin/change-management-settings"))
        .and(header("Authorization", "Bearer dap_tools_ci_key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "mode": "git_backed" })))
        .mount(&observer)
        .await;

    let base = Url::parse(&format!("{}/", observer.uri())).expect("url");
    let client = Client::new(base, TokenSource::static_token("dap_tools_ci_key".into()));
    let err = client
        .ensure_bundle_plan_allowed()
        .await
        .expect_err("git-backed tenant should block bundles");
    assert!(err.to_string().contains("git-backed change management"));
    assert!(err.to_string().contains("Bundle upload"));
}
