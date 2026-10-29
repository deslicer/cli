//! `deslicer upgrade readiness | preview | status | plan` against a mocked
//! Observer (refs #149). Asserts the read-only commands never hit the plan
//! route and that `plan` prints plan_id plus the next step.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SH1: &str = "11111111-1111-4111-8111-111111111111";
const SH2: &str = "22222222-2222-4222-8222-222222222222";
const PLAN: &str = "0e4f8a34-1111-4222-8333-444455556666";

async fn run(dir: &Path, observer: &str, args: &[&str]) -> Output {
    let dir = dir.to_path_buf();
    let observer = observer.to_string();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
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
            .arg("upgrade")
            .args(&args)
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}

async fn observer() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/hosts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "r1", "host_id": SH1, "hostname": "sh1"},
            {"id": "r2", "host_id": SH2, "hostname": "sh2"}
        ])))
        .mount(&server)
        .await;
    // Read-only commands must never create a plan.
    Mock::given(method("POST"))
        .and(path("/api/v1/splunk-packages/upgrade-rolling"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .named("plan route")
        .mount(&server)
        .await;
    server
}

const ROLLING: &[&str] = &[
    "--family",
    "upgrade_rolling_clusters",
    "--target",
    "sh1",
    "--target",
    SH2,
    "--to-version",
    "9.4.1",
    "--captain",
    "sh1",
    "--role-group",
    "searchhead",
];

fn rolling_body() -> Value {
    json!({
        "family": "upgrade_rolling_clusters",
        "host_ids": [SH1, SH2],
        "to_version": "9.4.1",
        "captain_hostname": "sh1",
        "enabled_role_groups": ["searchhead"]
    })
}

#[tokio::test]
async fn readiness_is_read_only_and_resolves_hostname() {
    let server = observer().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/splunk-packages/bootstrap/evaluate"))
        .and(body_json(json!({"host_id": SH1})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "path": "upgrade", "needs_readiness_plan": false, "reasons": [],
            "pin_version": "9.4.1", "installed_version": "9.2.0",
            "strict_x509_required": false, "strict_x509_passed": null,
            "captain_stability": "flapping"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        dir.path(),
        &server.uri(),
        &["readiness", "--target", "SH1", "--json"],
    )
    .await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["host_id"], SH1);
    assert_eq!(v["readiness"]["captain_stability"], "flapping");

    let out = run(
        dir.path(),
        &server.uri(),
        &["readiness", "--target", "nope"],
    )
    .await;
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test]
async fn preview_and_deprecated_status_alias() {
    let server = observer().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/splunk-packages/upgrade-rolling/schedule"))
        .and(body_json(rolling_body()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "family": "upgrade_rolling_clusters", "catalog_revision": "r1",
            "applied_overlay_ids": [], "to_version": "9.4.1", "publishable_wave_index": 0,
            "waves": [{"wave_index": 0, "step_id": "searchhead", "host_scope": "shc",
                       "serial_policy": "serial", "host_ids": [SH1, SH2],
                       "sub_batch_count": 2, "publishable": true, "outcome": "pending"}]
        })))
        .expect(2)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();

    let mut args = vec!["preview"];
    args.extend_from_slice(ROLLING);
    args.push("--json");
    let out = run(dir.path(), &server.uri(), &args).await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["waves"][0]["step_id"], "searchhead");

    let mut args = vec!["--output", "human", "status"];
    args.extend_from_slice(ROLLING);
    let out = run(dir.path(), &server.uri(), &args).await;
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("deprecated"));
    assert!(String::from_utf8_lossy(&out.stdout).contains("no plan was created"));
}

#[tokio::test]
async fn plan_prints_plan_id_and_next_step() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/hosts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"host_id": SH1, "hostname": "sh1"}, {"host_id": SH2, "hostname": "sh2"}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/splunk-packages/upgrade-rolling"))
        .and(body_json(rolling_body()))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "plan_id": PLAN, "family": "upgrade_rolling_clusters", "catalog_revision": "r1",
            "applied_overlay_ids": [], "phase_group_ids": ["g"], "phase_ids": ["a", "b"],
            "host_ids": [SH1, SH2], "filename": "splunk-9.4.1.tgz", "sha256": "ab",
            "staged_path": "/opt/x", "to_version": "9.4.1"
        })))
        .expect(2)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();

    let mut args = vec!["plan"];
    args.extend_from_slice(ROLLING);
    let out = run(dir.path(), &server.uri(), &args).await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.starts_with(&format!("plan_id: {PLAN}")));
    assert!(text.contains(&format!("deslicer change approve --plan-id {PLAN}")));

    args.push("--json");
    let out = run(dir.path(), &server.uri(), &args).await;
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["plan_id"], PLAN);
    assert_eq!(
        v["next_steps"][1],
        format!("deslicer change deploy --plan-id {PLAN}")
    );
}

#[tokio::test]
async fn invalid_args_exit_2_before_any_call() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    for args in [
        vec![
            "plan",
            "--family",
            "upgrade_non_clustered",
            "--to-version",
            "9.4.1",
        ],
        vec![
            "plan",
            "--family",
            "x",
            "--target",
            "sh1",
            "--to-version",
            "9.4.1",
        ],
        vec![
            "plan",
            "--family",
            "upgrade_non_clustered",
            "--target",
            "sh1",
            "--to-version",
            "latest",
        ],
        vec![
            "plan",
            "--family",
            "upgrade_non_clustered",
            "--target",
            "sh1",
            "--to-version",
            "9.4.1",
            "--yes",
        ],
        vec!["readiness", "--json"],
    ] {
        let out = run(dir.path(), &server.uri(), &args).await;
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}
