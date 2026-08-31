use super::*;
use reqwest::StatusCode;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const RUN_ID: &str = "11111111-1111-4111-8111-111111111111";

fn client_for(server: &MockServer) -> AgentClient {
    AgentClient {
        base: url::Url::parse(&server.uri()).expect("mock uri parses"),
        token: "test-session-token".into(),
        json: try_client().expect("json client"),
        streaming: try_streaming_client().expect("streaming client"),
    }
}

#[tokio::test]
async fn reads_a_run_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/cli/agents/runs/{RUN_ID}")))
        .and(header("authorization", "Bearer test-session-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "runId": RUN_ID,
            "agentId": "agent-1",
            "status": "succeeded",
            "conversationId": "conv-1",
            "errorCode": null,
            "startedAt": "2026-08-31T07:00:00.000Z",
            "finishedAt": "2026-08-31T07:00:09.000Z",
        })))
        .mount(&server)
        .await;

    let status = client_for(&server)
        .run_status(RUN_ID)
        .await
        .expect("status reads");

    assert_eq!(status.status, "succeeded");
    assert_eq!(status.conversation_id.as_deref(), Some("conv-1"));
    assert!(status.is_terminal());
}

#[tokio::test]
async fn reads_a_run_output() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/cli/agents/runs/{RUN_ID}/output")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "runId": RUN_ID,
            "agentId": "agent-1",
            "status": "succeeded",
            "conversationId": "conv-1",
            "errorCode": null,
            "startedAt": "2026-08-31T07:00:00.000Z",
            "finishedAt": "2026-08-31T07:00:09.000Z",
            "output": "the answer",
        })))
        .mount(&server)
        .await;

    let output = client_for(&server)
        .run_output(RUN_ID)
        .await
        .expect("output reads");

    assert_eq!(output.output.as_deref(), Some("the answer"));
    assert_eq!(output.status.status, "succeeded");
}

#[tokio::test]
async fn a_run_that_is_not_ours_reads_as_a_bad_argument() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/cli/agents/runs/{RUN_ID}")))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": "run_not_found",
            "message": "No such run.",
        })))
        .mount(&server)
        .await;

    let err = client_for(&server)
        .run_status(RUN_ID)
        .await
        .expect_err("should fail");

    assert!(err.to_string().contains("printed when a run starts"));
}

#[tokio::test]
async fn no_buffered_stream_is_not_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/cli/agents/runs/{RUN_ID}/stream")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let resumed = client_for(&server)
        .resume_run(RUN_ID)
        .await
        .expect("204 is an ordinary outcome");

    assert!(resumed.is_none());
}

#[tokio::test]
async fn a_buffered_stream_comes_back_open() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/cli/agents/runs/{RUN_ID}/stream")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: {\"type\":\"text-delta\",\"delta\":\"hi\"}\n\n"),
        )
        .mount(&server)
        .await;

    let resumed = client_for(&server)
        .resume_run(RUN_ID)
        .await
        .expect("resume succeeds")
        .expect("a stream is returned");

    assert_eq!(resumed.status(), StatusCode::OK);
}

#[tokio::test]
async fn lists_this_members_runs() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/cli/agents/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "runs": [{
                "runId": RUN_ID,
                "status": "succeeded",
                "agentId": "agent-1",
                "startedAt": "2026-08-31T11:00:00.000Z",
                "promptPreview": "check the fleet",
            }],
            "nextCursor": null,
        })))
        .mount(&server)
        .await;

    let body = client_for(&server)
        .list_runs(None, None, None)
        .await
        .expect("list");
    assert_eq!(body.runs.len(), 1);
    assert_eq!(
        body.runs[0].prompt_preview.as_deref(),
        Some("check the fleet")
    );
}

#[tokio::test]
async fn reads_the_latest_run() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/cli/agents/runs/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "runId": RUN_ID,
            "status": "running",
            "agentId": "agent-1",
            "startedAt": "2026-08-31T11:00:00.000Z",
            "promptPreview": "check the fleet",
        })))
        .mount(&server)
        .await;

    let latest = client_for(&server).latest_run().await.expect("latest");
    assert_eq!(latest.run_id, RUN_ID);
    assert_eq!(latest.status, "running");
}

#[test]
fn omits_agent_id_when_the_caller_wants_the_default() {
    let body = serde_json::to_value(RunRequestBody {
        agent_id: None,
        prompt: "hi",
        conversation_id: None,
    })
    .expect("json");
    assert_eq!(body, serde_json::json!({"prompt": "hi"}));
    assert!(body.get("agentId").is_none());
}

#[test]
fn run_path_rejects_an_id_that_could_reshape_the_url() {
    let err = run_path("../../admin", "/stream").expect_err("should reject");
    assert!(err.to_string().contains("not a run id"));
}

#[test]
fn run_path_builds_the_suffixed_endpoint() {
    let path = run_path("55555555-5555-4555-8555-555555555555", "/output").expect("path");
    assert_eq!(
        path,
        "api/cli/agents/runs/55555555-5555-4555-8555-555555555555/output"
    );
}
