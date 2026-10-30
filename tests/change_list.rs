//! `change list` pagination + filters (issue #224). Offline mocks only.

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

#[tokio::test]
async fn change_list_pages_legacy_array() {
    let server = MockServer::start().await;
    let plans: Vec<Value> = (0..60)
        .map(|i| {
            json!({
                "id": format!("row-{i}"),
                "plan_id": format!("plan-{i}"),
                "status": "draft",
                "name": format!("n{i}")
            })
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(plans))
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
    assert_eq!(v["partial"], false);
}

#[tokio::test]
async fn change_list_honours_server_page_shape() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .and(query_param("status", "pending_approval"))
        .and(query_param("limit", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [
                {"id":"r1","plan_id":"p1","status":"pending_approval","name":"a"}
            ],
            "next_cursor": "abc",
            "total": 1
        })))
        .mount(&server)
        .await;

    let config = TempDir::new().unwrap();
    let output = command(&server, &config)
        .args(["--status", "pending_approval", "--limit", "10"])
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["items"][0]["plan_id"], "p1");
    assert_eq!(v["next_cursor"], "abc");
    assert_eq!(v["total"], 1);
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
