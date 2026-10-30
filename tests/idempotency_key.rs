//! `--idempotency-key` on plan-creating commands (#228). Offline mocks only.

use std::process::Command;

use clap::CommandFactory;
use deslicer_cli::cli::Cli;
use serde_json::{json, Value};
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn clear_ci_keys(cmd: &mut Command) {
    for key in [
        "GITHUB_RUN_ID",
        "CI_PIPELINE_ID",
        "BUILD_BUILDID",
        "BITBUCKET_BUILD_NUMBER",
        "CIRCLE_BUILD_NUM",
        "DESLICER_IDEMPOTENCY_KEY",
        "DESLICER_DEV_TOKEN",
        "CI",
    ] {
        cmd.env_remove(key);
    }
}

#[test]
fn change_plan_help_documents_idempotency_key() {
    let mut cmd = Cli::command();
    let change = cmd.find_subcommand_mut("change").expect("change");
    let plan = change.find_subcommand_mut("plan").expect("plan");
    let mut buf = Vec::new();
    plan.write_long_help(&mut buf).expect("help");
    let help = String::from_utf8(buf).expect("utf8");
    assert!(
        help.contains("--idempotency-key"),
        "change plan help missing --idempotency-key:\n{help}"
    );
}

#[tokio::test]
async fn remediate_draft_sends_idempotency_key_and_marks_replay() {
    let server = MockServer::start().await;
    // Same Client Idempotency-Key path as `change plan` / `catalog plan` / `upgrade plan`.
    Mock::given(method("POST"))
        .and(path("/api/v1/remediate/draft"))
        .and(header("Idempotency-Key", "agent-retry-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Idempotency-Replayed", "true")
                .set_body_json(json!({
                    "plan": {
                        "id": "row-1",
                        "plan_id": "plan-1",
                        "status": "draft",
                        "name": "rem"
                    }
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let config = TempDir::new().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer"));
    clear_ci_keys(&mut cmd);
    let output = cmd
        .env("DESLICER_CONFIG_DIR", config.path())
        .env("DESLICER_TOKEN_STORE", "file")
        .env("OBSERVER_API_URL", server.uri())
        .env("DESLICER_API_TOKEN", "test-tools-key")
        .args([
            "--output",
            "json",
            "remediate",
            "draft",
            "--app",
            "demo",
            "--host",
            "h1",
            "--yes",
            "--idempotency-key",
            "agent-retry-1",
        ])
        .output()
        .expect("run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "status={} stderr={} stdout={}",
        output.status,
        stderr,
        stdout
    );
    let line = stdout
        .lines()
        .find(|l| l.contains("\"plan_id\"") && l.trim_start().starts_with('{'))
        .unwrap_or_else(|| panic!("expected plan JSON on stdout, got:\n{stdout}"));
    let v: Value = serde_json::from_str(line).expect("plan JSON");
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["plan_id"], "plan-1");
    assert_eq!(v["replayed"], true);
}
