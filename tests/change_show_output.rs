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
    json!({"id":"row-id", "plan_id":"87597ca8-ac36-409f-9b68-a4db995172f1", "status":"pending_approval",
        "name":"Update Splunk app", "summary":"Two configuration changes"})
}

async fn single(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v1/plans/87597ca8-ac36-409f-9b68-a4db995172f1"))
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
async fn single_output_human_without_json_metadata() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    // Piped stdout defaults to JSON (#221); request human explicitly.
    let text = stdout(command(&server, &config).args([
        "--output",
        "human",
        "--plan-id",
        "87597ca8-ac36-409f-9b68-a4db995172f1",
    ]));
    assert!(
        text.contains("Plan ID: 87597ca8-ac36-409f-9b68-a4db995172f1"),
        "{text}"
    );
    assert!(text.contains("Status: pending_approval"));
    assert!(text.contains("Name: Update Splunk app"));
    assert!(text.contains("Summary: Two configuration changes"));
    assert!(
        text.contains(
            "next: deslicer change status --plan-id 87597ca8-ac36-409f-9b68-a4db995172f1" // pragma: allowlist secret
        ),
        "{text}"
    );
    assert!(!text.contains('{'), "{text}");
}

#[tokio::test]
async fn single_piped_defaults_to_json() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    let text = stdout(
        command(&server, &config).args(["--plan-id", "87597ca8-ac36-409f-9b68-a4db995172f1"]),
    );
    let records: Vec<Value> = serde_json::Deserializer::from_str(&text)
        .into_iter()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(records[0]["schema_version"], 1);
    assert_eq!(
        records[0]["plan_id"],
        "87597ca8-ac36-409f-9b68-a4db995172f1"
    );
    assert_eq!(records[0]["status"], "pending_approval");
    assert_eq!(
        records[0]["next_command"],
        "deslicer change status --plan-id 87597ca8-ac36-409f-9b68-a4db995172f1" // pragma: allowlist secret
    );
    assert!(!records[0]["next_commands"].as_array().unwrap().is_empty());
    assert_eq!(records[0]["next_steps"], records[0]["next_commands"]);
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
    assert!(
        text.contains("87597ca8-ac36-409f-9b68-a4db995172f1  pending_approval  Update Splunk app")
    );
    assert!(text.contains("fallback-id  draft  -"));
    assert_eq!(text.lines().count(), 4);
    assert!(!text.contains('\u{001b}'));
}

#[tokio::test]
async fn empty_list_has_a_human_message_but_json_wraps_items() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/plans"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let config = TempDir::new().unwrap();
    assert_eq!(
        stdout(command(&server, &config).args(["--output", "human"])),
        "No change plans found.\n"
    );
    let text = stdout(command(&server, &config).args(["--output", "json"]));
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({"schema_version": 1, "items": []})
    );
}

#[tokio::test]
async fn single_json_preserves_plan_and_local_ci_records() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    let output = command(&server, &config)
        .args([
            "--plan-id",
            "87597ca8-ac36-409f-9b68-a4db995172f1",
            "--log-format",
            "json",
        ])
        .output()
        .unwrap();
    // A UUID plan id is accepted (exit 0, #281). Stdout is one JSON document (#264).
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let doc: Value = serde_json::from_str(&text).expect("stdout must be one JSON document");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["id"], "row-id");
    assert_eq!(doc["plan_id"], "87597ca8-ac36-409f-9b68-a4db995172f1");
    assert_eq!(
        doc["next_command"],
        "deslicer change status --plan-id 87597ca8-ac36-409f-9b68-a4db995172f1" // pragma: allowlist secret
    );
    // CI key stays off the primary document. The row id is `id`.
    assert!(doc.get("plan_row_id").is_none());
}

#[tokio::test]
async fn non_uuid_plan_id_exits_2_with_empty_stdout() {
    let server = MockServer::start().await;
    let config = TempDir::new().unwrap();
    let output = command(&server, &config)
        .args(["--output", "json", "--plan-id", "external-id"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "usage failure must not print a plan or CI record: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    let doc: Value = serde_json::from_str(stderr.trim()).expect("stderr must be one JSON document");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["error"]["code"], "usage.invalid");
    assert_eq!(doc["error"]["exit_code"], 2);
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
            .args([
                "--plan-id",
                "87597ca8-ac36-409f-9b68-a4db995172f1",
                "--log-format",
                "human",
            ])
            .env("GITHUB_ACTIONS", "true")
            .env("GITHUB_OUTPUT", &output_path)
            .env("GITHUB_STEP_SUMMARY", &summary_path),
    );
    assert!(!text.contains('{'), "{text}");
    let outputs = std::fs::read_to_string(output_path).unwrap();
    assert!(outputs.contains("plan_id=87597ca8-ac36-409f-9b68-a4db995172f1\n"));
    assert!(outputs.contains("plan_status=pending_approval\n"));
    assert!(std::fs::read_to_string(summary_path)
        .unwrap()
        .contains("87597ca8-ac36-409f-9b68-a4db995172f1"));
}

#[tokio::test]
async fn human_github_without_output_file_does_not_fall_back_to_json() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    let text = stdout(
        command(&server, &config)
            .args([
                "--output",
                "human",
                "--plan-id",
                "87597ca8-ac36-409f-9b68-a4db995172f1",
            ])
            .env("GITHUB_ACTIONS", "true"),
    );
    assert!(text.contains("Plan ID: 87597ca8-ac36-409f-9b68-a4db995172f1"));
    assert!(!text.contains('{'), "{text}");
}

#[tokio::test]
async fn populated_json_list_wraps_items_with_schema_version() {
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
        json!({"schema_version": 1, "items": [plan()]})
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
            .args([
                "--output",
                "human",
                "--plan-id",
                "87597ca8-ac36-409f-9b68-a4db995172f1",
            ])
            .env("GITLAB_CI", "true")
            .env("DESLICER_DOTENV_PATH", &dotenv),
    );
    assert!(!text.contains('{'));
    assert!(std::fs::read_to_string(dotenv)
        .unwrap()
        .contains("plan_id=87597ca8-ac36-409f-9b68-a4db995172f1\n"));
    let text = stdout(
        command(&server, &config)
            .args([
                "--output",
                "human",
                "--plan-id",
                "87597ca8-ac36-409f-9b68-a4db995172f1",
            ])
            .env("GITLAB_CI", "true"),
    );
    assert!(!text.contains('{'));
    let text = stdout(
        command(&server, &config)
            .args([
                "--output",
                "human",
                "--plan-id",
                "87597ca8-ac36-409f-9b68-a4db995172f1",
            ])
            .env("TF_BUILD", "true"),
    );
    assert!(text
        .contains("##vso[task.setvariable variable=plan_id]87597ca8-ac36-409f-9b68-a4db995172f1"));
    let text = stdout(
        command(&server, &config)
            .args([
                "--output",
                "human",
                "--plan-id",
                "87597ca8-ac36-409f-9b68-a4db995172f1",
            ])
            .env("BITBUCKET_BUILD_NUMBER", "1")
            .env("BITBUCKET_PIPE_STORAGE_DIR", config.path()),
    );
    assert!(!text.contains('{'));
    assert!(
        std::fs::read_to_string(config.path().join("deslicer-output.env"))
            .unwrap()
            .contains("plan_id=87597ca8-ac36-409f-9b68-a4db995172f1\n")
    );
}

#[tokio::test]
async fn human_single_reports_ci_write_failure() {
    let server = MockServer::start().await;
    single(&server).await;
    let config = TempDir::new().unwrap();
    let result = command(&server, &config)
        .args([
            "--output",
            "human",
            "--plan-id",
            "87597ca8-ac36-409f-9b68-a4db995172f1",
        ])
        .env("GITHUB_ACTIONS", "true")
        .env("GITHUB_OUTPUT", config.path())
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("output write failed"));
}
