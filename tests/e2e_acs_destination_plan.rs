//! `change plan --destination acs-stack:<stack>` against a mocked Observer:
//! create a Cloud plan, then project-plan with environment_name + apps.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ROW_ID: &str = "01890a5d-7777-7888-9999-aaaabbbbcccc";
const PLAN_ID: &str = "0e4f8a34-1111-4222-8333-444455556666";
const ENV_YAML: &str = "destinations:\n  - inventory_group: cloud\n    apps:\n      - source_path: apps/acme_indexes\n";

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

async fn run(dir: &Path, observer: &str, extra: &[&str]) -> Output {
    let dir = dir.to_path_buf();
    let observer = observer.to_string();
    let extra: Vec<String> = extra.iter().map(|s| s.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        Command::new(env!("CARGO_BIN_EXE_deslicer"))
            .env_clear()
            .env("HOME", &home)
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("OBSERVER_API_URL", format!("{observer}/"))
            .env("DESLICER_API_TOKEN", "dap_tools_test_key")
            .current_dir(&dir)
            .args(["change", "plan", "--destination", "acs-stack:scv-lab"])
            .args(&extra)
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn acs_destination_sends_environment_and_apps_and_returns_plan_id() {
    let observer = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plans"))
        .and(body_partial_json(json!({
            "stack_kind": "cloud",
            "acs_stack_name": "scv-lab",
            "source_type": "manual"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "success": true, "message": "ok",
            "plan": {"id": ROW_ID, "plan_id": PLAN_ID, "status": "draft"}
        })))
        .expect(1)
        .mount(&observer)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/acs/stacks/scv-lab/project-plan"))
        .and(body_partial_json(json!({
            "environment_name": "prod",
            "plan_id": ROW_ID,
            "mapping_source": "repo_yaml",
            "apps": [{
                "app_name": "acme_indexes",
                "inventory_group": "cloud",
                "source_path": "apps/acme_indexes",
                "files": {"default/indexes.conf": "[acme]\nhomePath = x\n"}
            }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "stack_name": "scv-lab", "environment_name": "prod",
            "expected_persisted": 1, "item_count": 1,
            "items": [{"action": "create", "resource_type": "index", "resource_id": "acme"}],
            "plan_id": ROW_ID, "plan_status": "pending_approval", "items_persisted": 1
        })))
        .expect(1)
        .mount(&observer)
        .await;

    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".deslicer/environments/prod.yml", ENV_YAML);
    write(
        dir.path(),
        "apps/acme_indexes/default/indexes.conf",
        "[acme]\nhomePath = x\n",
    );

    let out = run(
        dir.path(),
        &observer.uri(),
        &["--environment", "prod", "--json"],
    )
    .await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {stderr}");
    assert!(
        stderr.contains("environment prod resolved from --environment"),
        "{stderr}"
    );
    assert!(!stderr.contains("dap_tools_test_key"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .find(|l| l.contains("plan_id"))
        .expect("json line");
    let v: Value = serde_json::from_str(line).unwrap();
    assert_eq!(v["plan_id"], PLAN_ID);
    assert_eq!(v["status"], "pending_approval");
    assert_eq!(v["item_count"], 1);
    assert_eq!(v["apps"], json!(["acme_indexes"]));
}

#[tokio::test]
async fn unresolvable_environment_or_apps_exit_2_without_calling_observer() {
    let observer = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&observer)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let out = run(dir.path(), &observer.uri(), &["--environment", "prod"]).await;
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("environment \"prod\" not found"));

    write(
        dir.path(),
        ".deslicer/environments/prod.yml",
        "destinations:\n  - inventory_group: cloud\n    apps: []\n",
    );
    let out = run(dir.path(), &observer.uri(), &["--environment", "prod"]).await;
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no destination with source_path apps"));
}
