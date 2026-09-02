use deslicer_cli::observer_client::Client;
use deslicer_cli::token_source::TokenSource;
use serde_json::json;
use url::Url;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PLAN_ID: &str = "0e4f8a34-1111-4222-8333-444455556666";
const PLAN_ROW_ID: &str = "01890a5d-7777-7888-9999-aaaabbbbcccc";
const GROUP_ID: &str = "019f36d6-3f61-7eea-9417-7ac4a8a10f69";
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const REPO: &str = "https://github.com/acme/splunk-config";

#[tokio::test]
async fn create_plan_from_git_posts_observer_fields() {
    let observer = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plans"))
        .and(header("Authorization", "Bearer dap_tools_ci_key"))
        .and(body_json(json!({
            "source_type": "git",
            "repository_url": REPO,
            "commit_sha": SHA,
            "target_group_id": GROUP_ID,
            "name": "ci-token-plan"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "success": true,
            "message": "ok",
            "plan": {
                "id": PLAN_ROW_ID,
                "plan_id": PLAN_ID,
                "status": "draft"
            }
        })))
        .mount(&observer)
        .await;

    let base = Url::parse(&format!("{}/", observer.uri())).expect("url");
    let client = Client::new(base, TokenSource::static_token("dap_tools_ci_key".into()));
    let plan = client
        .create_plan_from_git(REPO, SHA, GROUP_ID, Some("ci-token-plan"))
        .await
        .expect("create");
    assert_eq!(plan.id, PLAN_ROW_ID);
    assert_eq!(plan.external_id(), PLAN_ID);
}

#[tokio::test]
async fn resolve_target_group_name_then_create_plan_from_git() {
    let observer = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/groups"))
        .and(header("Authorization", "Bearer dap_tools_ci_key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": GROUP_ID,
            "name": "search-heads",
            "display_name": "Search Heads",
            "member_count": 2
        }])))
        .mount(&observer)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plans"))
        .and(header("Authorization", "Bearer dap_tools_ci_key"))
        .and(body_json(json!({
            "source_type": "git",
            "repository_url": REPO,
            "commit_sha": SHA,
            "target_group_id": GROUP_ID
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "success": true,
            "message": "ok",
            "plan": {
                "id": PLAN_ROW_ID,
                "plan_id": PLAN_ID,
                "status": "pending_approval"
            }
        })))
        .mount(&observer)
        .await;

    let base = Url::parse(&format!("{}/", observer.uri())).expect("url");
    let client = Client::new(base, TokenSource::static_token("dap_tools_ci_key".into()));
    let groups = client.list_groups().await.expect("groups");
    let target_group_id =
        deslicer_cli::target_group::resolve_target_group_id("search-heads", &groups)
            .expect("resolve");
    assert_eq!(target_group_id, GROUP_ID);
    let plan = client
        .create_plan_from_git(REPO, SHA, &target_group_id, None)
        .await
        .expect("create");
    assert_eq!(plan.id, PLAN_ROW_ID);
    assert_eq!(plan.status, "pending_approval");
}

#[tokio::test]
async fn create_plan_from_git_reuses_existing_plan_on_duplicate_plan_409() {
    let observer = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plans"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "error": "duplicate_plan",
            "message": "An active plan already exists for this repository and commit SHA"
        })))
        .mount(&observer)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plans/search"))
        .and(header("Authorization", "Bearer dap_tools_ci_key"))
        .and(body_json(json!({
            "filters": {
                "repository_url": REPO,
                "commit_sha": SHA
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plans": [{
                "id": PLAN_ROW_ID,
                "plan_id": PLAN_ID,
                "status": "pending_approval"
            }],
            "total": 1,
            "limit": 100,
            "offset": 0
        })))
        .mount(&observer)
        .await;

    let base = Url::parse(&format!("{}/", observer.uri())).expect("url");
    let client = Client::new(base, TokenSource::static_token("dap_tools_ci_key".into()));
    let plan = client
        .create_plan_from_git(REPO, SHA, GROUP_ID, None)
        .await
        .expect("reuse");
    assert_eq!(plan.id, PLAN_ROW_ID);
    assert_eq!(plan.external_id(), PLAN_ID);
    assert_eq!(plan.status, "pending_approval");
}
