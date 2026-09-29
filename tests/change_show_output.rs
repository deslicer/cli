use std::process::Command;

use serde_json::{json, Value};
use tempfile::TempDir;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn command(server: &MockServer, config: &TempDir) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deslicer"));
    cmd.env("DESLICER_CONFIG_DIR", config.path())
        .env("DESLICER_TOKEN_STORE", "file")
        .env("OBSERVER_API_URL", server.uri())
        .env("DESLICER_API_TOKEN", "test-tools-key")
        .args(["change", "show"]);
    for key in [
        "DESLICER_DEV_TOKEN",
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "TF_BUILD",
        "BITBUCKET_BUILD_NUMBER",
        "GITHUB_OUTPUT",
        "GITHUB_STEP_SUMMARY",
        "DESLICER_DOTENV_PATH",
        "BITBUCKET_PIPE_STORAGE_DIR",
    ] {
        cmd.env_remove(key);
    }
    cmd
}

fn plan() -> Value {
    json!({"id":"row-id", "plan_id":"external-id", "status":"pending_approval",
        "name":"Update Splunk app", "summary":"Two configuration changes"})
}

async fn single(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v1/plans/external-id"))
        .and(header("Authorization", "Bearer test-tools-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(plan()))
        .mount(server)
        .await;
}

fn stdout(cmd: &mut Command) -> String {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[tokio::test]
async fn single_defaults_to_human_without_json_metadata() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    let text = stdout(command(&server, &config).args(["--plan-id", "external-id"]));
    assert!(text.contains("Plan ID: external-id"), "{text}");
    assert!(text.contains("Status: pending_approval"));
    assert!(text.contains("Name: Update Splunk app"));
    assert!(text.contains("Summary: Two configuration changes"));
    assert!(!text.contains('{'), "{text}");
}

#[tokio::test]
async fn list_human_handles_optional_fields_and_control_characters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .and(query_param("environment", "demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            plan(), {"id":"fallback-id", "status":"draft", "name":null},
            {"id":"row-3", "plan_id":"plan-3", "status":"draft", "name":"bad\nname\t\u{001b}[31m"}
        ])))
        .mount(&server)
        .await;
    let config = TempDir::new().unwrap();
    let text =
        stdout(command(&server, &config).args(["--environment", "demo", "--log-format", "human"]));
    assert!(text.starts_with("PLAN ID  STATUS  NAME\n"), "{text}");
    assert!(text.contains("external-id  pending_approval  Update Splunk app"));
    assert!(text.contains("fallback-id  draft  -"));
    assert_eq!(text.lines().count(), 4);
    assert!(!text.contains('\u{001b}'));
}

#[tokio::test]
async fn empty_list_has_a_human_message_but_json_stays_an_array() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let config = TempDir::new().unwrap();
    assert_eq!(
        stdout(&mut command(&server, &config)),
        "No change plans found.\n"
    );
    let text = stdout(command(&server, &config).args(["--log-format", "json"]));
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), json!([]));
}

#[tokio::test]
async fn single_json_preserves_plan_and_local_ci_records() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    let text = stdout(command(&server, &config).args([
        "--plan-id",
        "external-id",
        "--log-format",
        "json",
    ]));
    let records: Vec<Value> = serde_json::Deserializer::from_str(&text)
        .into_iter()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0], plan());
    assert_eq!(records[1]["plan_id"], "external-id");
    assert_eq!(records[1]["plan_row_id"], "row-id");
}

#[tokio::test]
async fn human_single_preserves_github_outputs_and_summary() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    let output_path = config.path().join("outputs");
    let summary_path = config.path().join("summary");
    let text = stdout(
        command(&server, &config)
            .args(["--plan-id", "external-id", "--log-format", "human"])
            .env("GITHUB_ACTIONS", "true")
            .env("GITHUB_OUTPUT", &output_path)
            .env("GITHUB_STEP_SUMMARY", &summary_path),
    );
    assert!(!text.contains('{'), "{text}");
    let outputs = std::fs::read_to_string(output_path).unwrap();
    assert!(outputs.contains("plan_id=external-id\n"));
    assert!(outputs.contains("plan_status=pending_approval\n"));
    assert!(std::fs::read_to_string(summary_path)
        .unwrap()
        .contains("external-id"));
}

#[tokio::test]
async fn human_github_without_output_file_does_not_fall_back_to_json() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    let text = stdout(
        command(&server, &config)
            .args(["--plan-id", "external-id"])
            .env("GITHUB_ACTIONS", "true"),
    );
    assert!(text.contains("Plan ID: external-id"));
    assert!(!text.contains('{'), "{text}");
}

#[tokio::test]
async fn populated_json_list_remains_one_array() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([plan()])))
        .mount(&server)
        .await;
    let config = TempDir::new().unwrap();
    let text = stdout(command(&server, &config).args(["--log-format", "json"]));
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!([plan()])
    );
}

#[tokio::test]
async fn human_preserves_other_ci_sinks_without_json_fallback() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    let dotenv = config.path().join("gitlab.env");
    let text = stdout(
        command(&server, &config)
            .args(["--plan-id", "external-id"])
            .env("GITLAB_CI", "true")
            .env("DESLICER_DOTENV_PATH", &dotenv),
    );
    assert!(!text.contains('{'));
    assert!(std::fs::read_to_string(dotenv)
        .unwrap()
        .contains("plan_id=external-id\n"));
    let text = stdout(
        command(&server, &config)
            .args(["--plan-id", "external-id"])
            .env("GITLAB_CI", "true"),
    );
    assert!(!text.contains('{'));
    let text = stdout(
        command(&server, &config)
            .args(["--plan-id", "external-id"])
            .env("TF_BUILD", "true"),
    );
    assert!(text.contains("##vso[task.setvariable variable=plan_id]external-id"));
    let text = stdout(
        command(&server, &config)
            .args(["--plan-id", "external-id"])
            .env("BITBUCKET_BUILD_NUMBER", "1")
            .env("BITBUCKET_PIPE_STORAGE_DIR", config.path()),
    );
    assert!(!text.contains('{'));
    assert!(
        std::fs::read_to_string(config.path().join("deslicer-output.env"))
            .unwrap()
            .contains("plan_id=external-id\n")
    );
}

#[tokio::test]
async fn human_single_reports_ci_write_failure() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    let result = command(&server, &config)
        .args(["--plan-id", "external-id"])
        .env("GITHUB_ACTIONS", "true")
        .env("GITHUB_OUTPUT", config.path())
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("output write failed"));
}
