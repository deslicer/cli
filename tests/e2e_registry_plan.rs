//! `change plan --from-registry <app>@<ver>` against a mocked Observer:
//! `POST /api/v1/plans` with `source_type=registry` (DAP #1395), then compile.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ROW_ID: &str = "01890a5d-7777-7888-9999-aaaabbbbcccc";
const PLAN_ID: &str = "0e4f8a34-1111-4222-8333-444455556666";
const GROUP_ID: &str = "5f0c2b8e-2222-4333-8444-555566667777";

async fn run(dir: &Path, observer: &str, extra: &[&str]) -> Output {
    let dir = dir.to_path_buf();
    let observer = observer.to_string();
    let extra: Vec<String> = extra.iter().map(|s| s.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer"));
        cmd.env_clear();
        // Windows sockets need SystemRoot (and friends) to initialise.
        for key in ["SystemRoot", "SYSTEMROOT", "windir", "TEMP", "TMP"] {
            if let Ok(v) = std::env::var(key) {
                cmd.env(key, v);
            }
        }
        cmd.env("HOME", &home)
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("OBSERVER_API_URL", format!("{observer}/"))
            .env("DESLICER_API_TOKEN", "dap_tools_test_key")
            .current_dir(&dir)
            .args(["change", "plan", "--from-registry", "Splunk_SA_CIM@6.1.0"])
            .args(&extra)
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn registry_plan_posts_registry_source_and_triggers_compile() {
    let observer = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plans"))
        .and(body_partial_json(json!({
            "source_type": "registry",
            "source_reference": "splunkbase:Splunk_SA_CIM/6.1.0",
            "target_group_id": GROUP_ID
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "success": true, "message": "Plan created in draft",
            "plan": {"id": ROW_ID, "plan_id": PLAN_ID, "status": "draft"}
        })))
        .expect(1)
        .mount(&observer)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/runners/compile/{ROW_ID}")))
        .and(body_partial_json(json!({"git_ref": "registry"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"status": "started"})))
        .expect(1)
        .mount(&observer)
        .await;
    // The old compare-registry call must not be used to create plans.
    Mock::given(method("POST"))
        .and(path("/api/v1/app-provenance/compare-registry"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&observer)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let out = run(
        dir.path(),
        &observer.uri(),
        &["--target-group", GROUP_ID, "--no-wait"],
    )
    .await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {stderr}");
    assert!(
        stderr.contains("splunkbase:Splunk_SA_CIM/6.1.0"),
        "{stderr}"
    );
    assert!(!stderr.contains("dap_tools_test_key"));
    assert!(String::from_utf8_lossy(&out.stdout).contains(PLAN_ID));
}

#[tokio::test]
async fn registry_plan_without_target_group_exits_2_before_create() {
    let observer = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plans"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&observer)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let out = run(dir.path(), &observer.uri(), &["--no-wait"]).await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains("needs a target group"), "{stderr}");
}
