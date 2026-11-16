//! Offline plan-runner path: enroll → worker recipe → draft plan → approve.
//!
//! The fake Observer and portal stand in for a lab tenant. Nothing here
//! approves a real plan or uses a real credential. Live workshop verification
//! still needs a device session and a worker host (see docs/plan-runner-e2e.md).

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ROW_ID: &str = "01890a5d-7777-7888-9999-aaaabbbbcccc";
const PLAN_ID: &str = "0e4f8a34-1111-4222-8333-444455556666";
const GROUP_ID: &str = "019f36d6-3f61-7eea-9417-7ac4a8a10f69";
const JTI: &str = "01890a5d-ac11-7eea-9417-7ac4a8a10f69";
const TENANT: &str = "01890a5d-ac22-7eea-9417-7ac4a8a10f69";

fn write_session(home: &Path, observer: &str) {
    let dir = home.join("config");
    std::fs::create_dir_all(&dir).unwrap();
    let body = format!(
        "cli_session_token = \"dslcli_{token}\"\n\
         expires_at = \"2099-01-01T00:00:00.000Z\"\n\
         tenant_id = \"tenant-lab\"\n\
         display_name = \"Lab\"\n\
         observer_api_url = \"{observer}/\"\n",
        token = "ab".repeat(32),
    );
    std::fs::write(dir.join("credentials.toml"), body).unwrap();
}

fn cli(home: &Path, observer: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer")); // pragma: allowlist secret
    cmd.env_clear();
    // Windows sockets need SystemRoot (and friends) to initialise.
    for key in ["SystemRoot", "SYSTEMROOT", "windir", "TEMP", "TMP"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
    cmd.env("HOME", home)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("DESLICER_CONFIG_DIR", home.join("config")) // pragma: allowlist secret
        .env("DESLICER_TOKEN_STORE", "file") // pragma: allowlist secret
        .env("DESLICER_API_URL", format!("{observer}/")) // pragma: allowlist secret
        .args(["--ci-platform", "local", "--output", "json", "--no-input"]);
    cmd
}

fn stdout_json(output: &std::process::Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut parsed = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                parsed.push(value);
            }
        }
    }
    parsed
        .into_iter()
        .find(|value| value.get("plan_id").is_some() || value.get("snippet").is_some())
        .unwrap_or_else(|| {
            panic!(
                "no plan/snippet json in stdout: {stdout}\nstderr {}",
                String::from_utf8_lossy(&output.stderr)
            )
        })
}

fn stderr_error(output: &std::process::Output) -> Value {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('{'))
        .unwrap_or("");
    serde_json::from_str(line).unwrap_or_else(|err| panic!("stderr json ({err}): {stderr}"))
}

#[tokio::test]
async fn enroll_worker_draft_and_approve_against_a_fake_backend() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/enrollment-tokens"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "token": "test-enroll-token",
            "jti": JTI,
            "tenant_id": TENANT,
            "max_hosts": 1,
            "expires_at": "2099-01-01T00:00:00Z",
            "api_key_ttl_days": 7,
            "bind_to_host_id": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/cli/worker-install-snippet"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "snippet": "echo install-worker"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plan-sources/bundles"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "bundle-1",
            "sha256": "abc",
            "size_bytes": 10
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plans"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "success": true,
            "message": "ok",
            "plan": {"id": ROW_ID, "plan_id": PLAN_ID, "status": "compile_pending"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/runners/compile/{ROW_ID}")))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": ROW_ID, "plan_id": PLAN_ID, "status": "pending_approval"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}/approve")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": ROW_ID, "plan_id": PLAN_ID, "status": "approved"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    std::fs::write(work.path().join("indexes.conf"), "[main]\n").unwrap();
    write_session(home.path(), &server.uri());

    let token_path = home.path().join("enroll.token");
    let enroll = cli(home.path(), &server.uri())
        .args([
            "enroll",
            "create",
            "--purpose",
            "bootstrap",
            "--name",
            "workshop-smoke",
            "--write-file",
            token_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(enroll.status.code(), Some(0), "{enroll:?}");
    assert!(std::fs::read_to_string(&token_path)
        .unwrap()
        .contains("test-enroll-token"));

    let worker = cli(home.path(), &server.uri())
        .args([
            "worker",
            "instructions",
            "--format",
            "shell",
            "--token-file",
            token_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(worker.status.code(), Some(0), "{worker:?}");
    let snippet = stdout_json(&worker);
    assert!(snippet["snippet"]
        .as_str()
        .unwrap()
        .contains("install-worker"));

    let plan = cli(home.path(), &server.uri())
        .env("OBSERVER_API_URL", format!("{}/", server.uri()))
        .env("DESLICER_API_TOKEN", "dap_tools_test_key") // pragma: allowlist secret
        .current_dir(work.path())
        .args([
            "change",
            "plan",
            "--source-dir",
            work.path().to_str().unwrap(),
            "--target-group",
            GROUP_ID,
            "--name",
            "plan-runner e2e",
        ])
        .output()
        .unwrap();
    assert_eq!(plan.status.code(), Some(0), "{plan:?}");
    let drafted = stdout_json(&plan);
    assert_eq!(drafted["plan_id"], PLAN_ID);
    assert_eq!(drafted["status"], "pending_approval");
    let next = drafted["next_commands"].to_string();
    assert!(
        next.contains("change approve"),
        "next steps should include approve: {drafted}"
    );

    let approve = cli(home.path(), &server.uri())
        .env("OBSERVER_API_URL", format!("{}/", server.uri()))
        .env("DESLICER_API_TOKEN", "dap_tools_test_key") // pragma: allowlist secret
        .args(["change", "approve", "--plan-id", PLAN_ID])
        .output()
        .unwrap();
    assert_eq!(approve.status.code(), Some(0), "{approve:?}");
    assert_eq!(stdout_json(&approve)["status"], "approved");
}

#[tokio::test]
async fn enroll_without_a_session_exits_auth() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let out = cli(home.path(), &server.uri())
        .args([
            "enroll",
            "create",
            "--purpose",
            "bootstrap",
            "--write-file",
            home.path().join("t").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4), "{out:?}");
    let err = stderr_error(&out);
    assert_eq!(err["error"]["exit_code"], 4);
    assert!(err["error"]["hint"].is_string() || err["error"]["next_command"].is_string());
}

#[tokio::test]
async fn worker_backend_unavailable_exits_10() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/cli/worker-install-snippet"))
        .respond_with(ResponseTemplate::new(503).set_body_string("unavailable"))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    write_session(home.path(), &server.uri());
    let out = cli(home.path(), &server.uri())
        .args(["worker", "instructions", "--format", "shell"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10), "{out:?}");
    assert_eq!(stderr_error(&out)["error"]["exit_code"], 10);
}

#[tokio::test]
async fn empty_plan_is_a_successful_no_changes_result() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plan-sources/bundles"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "bundle-1", "sha256": "abc", "size_bytes": 4
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/plans"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "success": true,
            "message": "ok",
            "plan": {"id": ROW_ID, "plan_id": PLAN_ID, "status": "compile_pending"}
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v1/runners/compile/{ROW_ID}")))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/plans/{PLAN_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": ROW_ID, "plan_id": PLAN_ID, "status": "no_changes"
        })))
        .mount(&server)
        .await;

    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    std::fs::write(work.path().join("app.conf"), "x").unwrap();
    let out = cli(home.path(), &server.uri())
        .env("OBSERVER_API_URL", format!("{}/", server.uri()))
        .env("DESLICER_API_TOKEN", "dap_tools_test_key") // pragma: allowlist secret
        .args([
            "change",
            "plan",
            "--source-dir",
            work.path().to_str().unwrap(),
            "--target-group",
            GROUP_ID,
            "--name",
            "empty",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(stdout_json(&out)["status"], "no_changes");
}
