//! HTTP client for the deslicer-ai CLI agent endpoints.
//!
//! Agent runs live on deslicer-ai, not on the Observer management plane, so
//! these calls go to `--deslicer-api-url` with the device-session bearer —
//! not to `--observer-api-url` with a tools key.

use serde::{Deserialize, Serialize};

use crate::device_flow::join_api;
use crate::errors::CliError;
use crate::http::{assert_url_allowed, try_client, try_streaming_client};
use crate::token_store::load_active_session;
use crate::Ctx;

use super::ids::parse_run_id;

const LIST_PATH: &str = "api/cli/agents";
const RUN_PATH: &str = "api/cli/agents/runs";

/// Header the server echoes so a run can be correlated with its ledger row.
const RUN_ID_HEADER: &str = "x-deslicer-run-id";
const CONVERSATION_ID_HEADER: &str = "x-deslicer-conversation-id";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub visibility: String,
}

#[derive(Debug, Deserialize)]
struct AgentListResponse {
    #[serde(default)]
    agents: Vec<AgentSummary>,
}

#[derive(Debug, Serialize)]
struct RunRequestBody<'a> {
    #[serde(rename = "agentId")]
    agent_id: &'a str,
    prompt: &'a str,
    #[serde(rename = "conversationId", skip_serializing_if = "Option::is_none")]
    conversation_id: Option<&'a str>,
}

/// A started run: response headers plus the still-open body.
pub struct StartedRun {
    pub run_id: Option<String>,
    pub conversation_id: Option<String>,
    pub response: reqwest::Response,
}

/// Where a run got to, as recorded in the server's ledger.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RunStatus {
    #[serde(rename = "runId")]
    pub run_id: String,
    pub status: String,
    #[serde(default, rename = "conversationId")]
    pub conversation_id: Option<String>,
    #[serde(default, rename = "errorCode")]
    pub error_code: Option<String>,
}

impl RunStatus {
    /// Whether the run has stopped moving, either way.
    pub fn is_terminal(&self) -> bool {
        self.status != "running"
    }
}

/// A run's status plus whatever answer has been persisted for it.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RunOutput {
    #[serde(flatten)]
    pub status: RunStatus,
    #[serde(default)]
    pub output: Option<String>,
}

pub struct AgentClient {
    base: url::Url,
    token: String,
    json: reqwest::Client,
    streaming: reqwest::Client,
}

impl AgentClient {
    /// Builds a client from the stored device session.
    pub fn from_ctx(ctx: &Ctx) -> Result<Self, CliError> {
        let session = load_active_session()?.ok_or_else(|| {
            CliError::Other(
                "no active device session. Run `deslicer auth login` first \
                 (agent runs are not available with a static API token)."
                    .into(),
            )
        })?;
        Ok(Self {
            base: ctx.deslicer_api_url.clone(),
            token: session.cli_session_token,
            json: try_client()?,
            streaming: try_streaming_client()?,
        })
    }

    pub async fn list_agents(&self) -> Result<Vec<AgentSummary>, CliError> {
        let url = self.endpoint(LIST_PATH)?;
        let response = self
            .json
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("list agents: {e}")))?;

        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        let body: AgentListResponse = response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode agent list: {e}")))?;
        Ok(body.agents)
    }

    /// Opens a run and returns before the first token.
    ///
    /// Only the response head is awaited here. The server writes an SSE
    /// preamble byte immediately so this resolves in roughly one round trip,
    /// even though the model has not produced anything yet.
    pub async fn start_run(
        &self,
        agent_id: &str,
        prompt: &str,
        conversation_id: Option<&str>,
        idempotency_key: &str,
    ) -> Result<StartedRun, CliError> {
        let url = self.endpoint(RUN_PATH)?;
        let response = self
            .streaming
            .post(url)
            .bearer_auth(&self.token)
            .header("accept", "text/event-stream")
            .header("idempotency-key", idempotency_key)
            .header("x-deslicer-cli-version", env!("CARGO_PKG_VERSION"))
            .json(&RunRequestBody {
                agent_id,
                prompt,
                conversation_id,
            })
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("start agent run: {e}")))?;

        let run_id = header_value(&response, RUN_ID_HEADER);
        let conversation_id = header_value(&response, CONVERSATION_ID_HEADER);

        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        Ok(StartedRun {
            run_id,
            conversation_id,
            response,
        })
    }

    pub async fn run_status(&self, run_id: &str) -> Result<RunStatus, CliError> {
        self.get_json(&run_path(run_id, "")?, "read run status")
            .await
    }

    pub async fn run_output(&self, run_id: &str) -> Result<RunOutput, CliError> {
        self.get_json(&run_path(run_id, "/output")?, "read run output")
            .await
    }

    /// Reattaches to a run's live output.
    ///
    /// `None` means there is nothing to attach to — the deployment buffers no
    /// streams, or this one has already been consumed. That is an ordinary
    /// outcome, not a failure: the caller polls the output instead.
    pub async fn resume_run(&self, run_id: &str) -> Result<Option<reqwest::Response>, CliError> {
        let url = self.endpoint(&run_path(run_id, "/stream")?)?;
        let response = self
            .streaming
            .get(url)
            .bearer_auth(&self.token)
            .header("accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("resume agent run: {e}")))?;

        if response.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        Ok(Some(response))
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> Result<T, CliError> {
        let url = self.endpoint(path)?;
        let response = self
            .json
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("{what}: {e}")))?;

        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode {what}: {e}")))
    }

    fn endpoint(&self, path: &str) -> Result<url::Url, CliError> {
        let url = join_api(&self.base, path)?;
        assert_url_allowed(&url)?;
        Ok(url)
    }

    async fn error_from(&self, response: reqwest::Response) -> CliError {
        let status = response.status();
        let retry_after = retry_after_seconds(&response);
        let body = response.text().await.unwrap_or_default();
        map_error_body(status, &body, retry_after)
    }
}

/// Builds a run-scoped path.
///
/// The id is interpolated into a URL, so it is checked against the shape the
/// server issues rather than trusted to be free of path separators. A typo'd
/// id should read as a bad argument here, not as a request to some other
/// endpoint.
fn run_path(run_id: &str, suffix: &str) -> Result<String, CliError> {
    parse_run_id(run_id)?;
    Ok(format!("{RUN_PATH}/{run_id}{suffix}"))
}

fn header_value(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn retry_after_seconds(response: &reqwest::Response) -> u64 {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(30)
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// Maps the server's error contract onto a `CliError`, and therefore onto an
/// exit code. The `error` codes are a stable interface — see
/// `lib/integrations/cli-device/agent-run/errors.ts` in deslicer-ai.
pub fn map_error_body(status: reqwest::StatusCode, body: &str, retry_after_secs: u64) -> CliError {
    let parsed: Option<ErrorBody> = serde_json::from_str(body).ok();
    let code = parsed.as_ref().and_then(|b| b.error.as_deref());
    let message = parsed
        .as_ref()
        .and_then(|b| b.message.as_deref())
        .filter(|m| !m.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("agent request failed (HTTP {})", status.as_u16()));

    match code {
        Some("too_many_runs") => CliError::RateLimited { retry_after_secs },
        Some("unauthorized") => CliError::Other(format!(
            "{message}\nRun `deslicer auth login` to start a new device session."
        )),
        Some("agent_not_found") => CliError::Other(format!(
            "{message}\nRun `deslicer agent list` to see the agents you can run."
        )),
        Some("run_in_progress") | Some("run_already_completed") => CliError::Other(message),
        Some("run_not_found") => CliError::Other(format!(
            "{message}\nRun ids are printed when a run starts, and are scoped to the \
             account that started them."
        )),
        Some("run_failed") => CliError::AgentRunFailed(message),
        Some(_) => CliError::Other(message),
        // No parsable body: fall back to the status class. A 5xx or 429 from
        // an intermediate proxy never reaches the handler's error contract.
        None if status == reqwest::StatusCode::TOO_MANY_REQUESTS => {
            CliError::RateLimited { retry_after_secs }
        }
        None if status.is_server_error() => CliError::BackendUnavailable(status.to_string()),
        None => CliError::Other(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const RUN_ID: &str = "11111111-1111-4111-8111-111111111111";

    /// Builds a client pointed at a mock server.
    ///
    /// Bypasses `from_ctx` so the tests do not need a device session in the
    /// host keyring.
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

        // The server deliberately does not say whether the run exists, so the
        // CLI explains the two things the caller can check instead.
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

        // Signals "poll the output instead", not "the run is unreachable".
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

    #[test]
    fn maps_too_many_runs_to_rate_limited() {
        let err = map_error_body(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":"too_many_runs","message":"Too many runs"}"#,
            17,
        );
        assert!(matches!(
            err,
            CliError::RateLimited {
                retry_after_secs: 17
            }
        ));
        assert_eq!(err.exit_code(), 9);
    }

    #[test]
    fn maps_run_failed_to_its_own_exit_code() {
        let err = map_error_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"error":"run_failed","message":"The run could not be started."}"#,
            30,
        );
        assert!(matches!(err, CliError::AgentRunFailed(_)));
        assert_eq!(err.exit_code(), 13);
    }

    #[test]
    fn unauthorized_points_at_the_login_command() {
        let err = map_error_body(
            StatusCode::UNAUTHORIZED,
            r#"{"error":"unauthorized","message":"CLI session expired."}"#,
            30,
        );
        assert!(err.to_string().contains("deslicer auth login"));
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn agent_not_found_points_at_the_list_command() {
        let err = map_error_body(
            StatusCode::NOT_FOUND,
            r#"{"error":"agent_not_found","message":"No such agent."}"#,
            30,
        );
        assert!(err.to_string().contains("deslicer agent list"));
    }

    #[test]
    fn unknown_code_keeps_the_server_message() {
        let err = map_error_body(
            StatusCode::BAD_REQUEST,
            r#"{"error":"invalid_request","message":"prompt is required"}"#,
            30,
        );
        assert_eq!(err.to_string(), "prompt is required");
    }

    #[test]
    fn html_error_page_from_a_proxy_falls_back_to_the_status_class() {
        let err = map_error_body(
            StatusCode::BAD_GATEWAY,
            "<html><body>502 Bad Gateway</body></html>",
            30,
        );
        assert!(matches!(err, CliError::BackendUnavailable(_)));
        assert_eq!(err.exit_code(), 10);
    }

    #[test]
    fn proxy_429_without_a_body_is_still_rate_limited() {
        let err = map_error_body(StatusCode::TOO_MANY_REQUESTS, "", 5);
        assert!(matches!(
            err,
            CliError::RateLimited {
                retry_after_secs: 5
            }
        ));
    }

    #[test]
    fn run_path_rejects_an_id_that_could_reshape_the_url() {
        // The id lands in a URL path; a traversal segment must read as a bad
        // argument rather than as a request to a different endpoint.
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

    #[test]
    fn run_status_reads_terminality_off_the_status_field() {
        let running: RunStatus =
            serde_json::from_str(r#"{"runId":"r","status":"running"}"#).expect("parse");
        assert!(!running.is_terminal());

        let done: RunStatus =
            serde_json::from_str(r#"{"runId":"r","status":"failed","errorCode":"abandoned"}"#)
                .expect("parse");
        assert!(done.is_terminal());
        assert_eq!(done.error_code.as_deref(), Some("abandoned"));
    }

    #[test]
    fn run_output_flattens_the_status_alongside_the_answer() {
        let parsed: RunOutput = serde_json::from_str(
            r#"{"runId":"r","status":"succeeded","conversationId":"c","output":"done"}"#,
        )
        .expect("parse");
        assert_eq!(parsed.status.status, "succeeded");
        assert_eq!(parsed.status.conversation_id.as_deref(), Some("c"));
        assert_eq!(parsed.output.as_deref(), Some("done"));
    }

    #[test]
    fn run_output_tolerates_an_answer_that_is_not_written_yet() {
        let parsed: RunOutput =
            serde_json::from_str(r#"{"runId":"r","status":"running"}"#).expect("parse");
        assert!(parsed.output.is_none());
    }

    #[test]
    fn blank_message_falls_back_to_the_status_code() {
        let err = map_error_body(
            StatusCode::FORBIDDEN,
            r#"{"error":"billing_blocked","message":"  "}"#,
            30,
        );
        assert!(err.to_string().contains("403"));
    }
}
