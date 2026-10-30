//! `--idempotency-key` on plan-creating commands (#228). Offline mocks only.

use std::process::Command;

use clap::CommandFactory;
use deslicer_cli::cli::Cli;
use deslicer_cli::observer_client::Client;
use deslicer_cli::token_source::TokenSource;
use serde_json::{json, Value};
use tempfile::TempDir;
use url::Url;
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

fn clear_ci_keys(cmd: &mut Command) {
    for key in [
        "GITHUB_RUN_ID",
        "CI_PIPELINE_ID",
        "BUILD_BUILDID",
        "BITBUCKET_BUILD_NUMBER",
        "CIRCLE_BUILD_NUM",
        "GITHUB_RUN_ATTEMPT",
        "GITHUB_JOB",
        "DESLICER_IDEMPOTENCY_KEY",
        "DESLICER_DEV_TOKEN",
        "CI",
    ] {
        cmd.env_remove(key);
    }
}

struct NoIdempotencyKey;

impl Match for NoIdempotencyKey {
    fn matches(&self, request: &Request) -> bool {
        !request.headers.contains_key("idempotency-key")
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

#[tokio::test]
async fn idempotency_key_only_on_plan_create_not_upload_or_compile() {
    // Bundle flow makes three POSTs; only plan-create may carry Idempotency-Key (#228 review).
    let server = MockServer::start().await;
    let plan_row = "01890a5d-7777-7888-9999-aaaabbbbcccc";
    let plan_id = "0e4f8a34-1111-4222-8333-444455556666";
    let group = "019f36d6-3f61-7eea-9417-7ac4a8a10f69";
    let bundle_id = "bundle-1";

    Mock::given(method("POST"))
        .and(path("/api/v1/plan-sources/bundles"))
        .and(NoIdempotencyKey)
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": bundle_id,
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "size_bytes": 4
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/plans"))
        .and(header_exists("Idempotency-Key"))
        .and(header("Idempotency-Key", "bundle-create-1"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "success": true,
            "message": "ok",
            "plan": {
                "id": plan_row,
                "plan_id": plan_id,
                "status": "draft"
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/api/v1/runners/compile/{plan_row}")))
        .and(NoIdempotencyKey)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .expect(1)
        .mount(&server)
        .await;

    let base = Url::parse(&format!("{}/", server.uri())).expect("url");
    let client = Client::new(base, TokenSource::static_token("dap_tools_ci_key".into()))
        .with_idempotency_key(Some("bundle-create-1".into()));
    let uploaded = client
        .upload_bundle(
            b"gzip".to_vec(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            None,
        )
        .await
        .expect("upload");
    assert_eq!(uploaded.id, bundle_id);
    let plan = client
        .create_plan_from_bundle(&uploaded.id, group, Some("airgap"))
        .await
        .expect("create");
    assert_eq!(plan.id, plan_row);
    client
        .trigger_compile(&plan.id, "bundle")
        .await
        .expect("compile");
}
