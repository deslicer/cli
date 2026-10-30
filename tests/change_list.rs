//! `change list` pagination + filters (issue #224). Offline mocks only.
//!
//! Observer contract: `GET /api/v1/plans?limit=&offset=` + `X-Total-Count`.

use std::process::Command;

use serde_json::{json, Value};
use tempfile::TempDir;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn command(server: &MockServer, config: &TempDir) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer"));
    cmd.env("DESLICER_CONFIG_DIR", config.path())
        .env("DESLICER_TOKEN_STORE", "file")
        .env("OBSERVER_API_URL", server.uri())
        .env("DESLICER_API_TOKEN", "test-tools-key")
        .args(["--output", "json", "change", "list"]);
    for key in [
        "DESLICER_DEV_TOKEN",
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "TF_BUILD",
        "BITBUCKET_BUILD_NUMBER",
        "CI",
    ] {
        cmd.env_remove(key);
    }
    cmd
}

fn plan_row(i: usize, status: &str) -> Value {
    json!({
        "id": format!("row-{i}"),
        "plan_id": format!("plan-{i}"),
        "status": status,
        "name": format!("n{i}"),
        "created_at": format!("2026-01-{:02}T00:00:00Z", (i % 28) + 1),
        "updated_at": format!("2026-01-{:02}T00:00:00Z", (i % 28) + 1),
    })
}

#[tokio::test]
async fn change_list_sends_offset_and_reads_total_count() {
    let server = MockServer::start().await;
    let page: Vec<Value> = (0..50).map(|i| plan_row(i, "draft")).collect();
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .and(query_param("limit", "50"))
        .and(query_param("offset", "0"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Total-Count", "120")
                .set_body_json(page),
        )
        .mount(&server)
        .await;

    let config = TempDir::new().unwrap();
    let output = command(&server, &config)
        .arg("--limit")
        .arg("50")
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["items"].as_array().unwrap().len(), 50);
    assert_eq!(v["next_cursor"], "50");
    assert_eq!(v["total"], 120);
    assert_eq!(v["partial"], false);
}

#[tokio::test]
async fn change_list_page_two_uses_offset_cursor() {
    let server = MockServer::start().await;
    let page: Vec<Value> = (50..100).map(|i| plan_row(i, "draft")).collect();
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .and(query_param("limit", "50"))
        .and(query_param("offset", "50"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Total-Count", "120")
                .set_body_json(page),
        )
        .expect(1)
        .mount(&server)
        .await;

    let config = TempDir::new().unwrap();
    let output = command(&server, &config)
        .args(["--limit", "50", "--cursor", "50"])
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 50);
    assert_eq!(v["items"][0]["plan_id"], "plan-50");
    assert_eq!(v["next_cursor"], "100");
    assert_eq!(v["total"], 120);
}

#[tokio::test]
async fn change_list_last_page_clears_next_cursor() {
    let server = MockServer::start().await;
    let page: Vec<Value> = (100..120).map(|i| plan_row(i, "draft")).collect();
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .and(query_param("limit", "50"))
        .and(query_param("offset", "100"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Total-Count", "120")
                .set_body_json(page),
        )
        .mount(&server)
        .await;

    let config = TempDir::new().unwrap();
    let output = command(&server, &config)
        .args(["--limit", "50", "--cursor", "100"])
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 20);
    assert!(v["next_cursor"].is_null());
    assert_eq!(v["total"], 120);
}

#[tokio::test]
async fn change_list_default_auto_paginates_to_completion() {
    let server = MockServer::start().await;
    let page0: Vec<Value> = (0..500).map(|i| plan_row(i, "draft")).collect();
    let page1: Vec<Value> = (500..620).map(|i| plan_row(i, "draft")).collect();
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .and(query_param("limit", "500"))
        .and(query_param("offset", "0"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Total-Count", "620")
                .set_body_json(page0),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .and(query_param("limit", "500"))
        .and(query_param("offset", "500"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Total-Count", "620")
                .set_body_json(page1),
        )
        .expect(1)
        .mount(&server)
        .await;

    let config = TempDir::new().unwrap();
    let output = command(&server, &config).output().expect("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 620);
    assert!(v["next_cursor"].is_null());
    assert_eq!(v["total"], 620);
}

#[tokio::test]
async fn change_list_does_not_send_ignored_filter_query_params() {
    let server = MockServer::start().await;
    // Mixed statuses across enough rows that a server-side ignore of filters
    // would still return drafts; client filter must keep only pending_approval.
    let page: Vec<Value> = (0..10)
        .map(|i| {
            let status = if i % 2 == 0 {
                "pending_approval"
            } else {
                "draft"
            };
            plan_row(i, status)
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .and(query_param("limit", "500"))
        .and(query_param("offset", "0"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Total-Count", "10")
                .set_body_json(page),
        )
        .expect(1)
        .mount(&server)
        .await;

    let config = TempDir::new().unwrap();
    let output = command(&server, &config)
        .args(["--status", "pending_approval", "--limit", "50"])
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 5);
    assert!(items.iter().all(|p| p["status"] == "pending_approval"));

    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1);
    let query = requests[0].url.query().unwrap_or("");
    assert!(query.contains("limit=500"));
    assert!(query.contains("offset=0"));
    assert!(
        !query.contains("status="),
        "Observer list ignores status; must not send it as a query param: {query}"
    );
    assert!(
        !query.contains("cursor="),
        "must send offset, not cursor: {query}"
    );
    assert!(
        !query.contains("since="),
        "must not send since as a query param: {query}"
    );
}

#[tokio::test]
async fn change_list_rejects_invalid_since() {
    let server = MockServer::start().await;
    let config = TempDir::new().unwrap();
    let output = command(&server, &config)
        .args(["--since", "yesterday"])
        .output()
        .expect("run");
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn change_list_missing_route_exits_15() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"detail":"not found"})))
        .mount(&server)
        .await;

    let config = TempDir::new().unwrap();
    let output = command(&server, &config).output().expect("run");
    assert_eq!(
        output.status.code(),
        Some(15),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn change_list_help_documents_pagination() {
    use clap::CommandFactory;
    use deslicer_cli::cli::Cli;

    let mut cmd = Cli::command();
    let change = cmd.find_subcommand_mut("change").expect("change");
    let list = change.find_subcommand_mut("list").expect("list");
    let mut buf = Vec::new();
    list.write_long_help(&mut buf).expect("help");
    let help = String::from_utf8(buf).expect("utf8");
    assert!(help.contains("--limit"));
    assert!(help.contains("--cursor"));
    assert!(help.contains("--status"));
}
