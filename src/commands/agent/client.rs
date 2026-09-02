//! HTTP client for the deslicer-ai CLI agent endpoints.
//!
//! Agent runs live on deslicer-ai, not on the Observer management plane, so
//! these calls go to `--deslicer-api-url` with the device-session bearer —
//! not to `--observer-api-url` with a tools key.

use serde::{Deserialize, Serialize};

use crate::device_flow::join_api;
use crate::errors::CliError;
use crate::http::{assert_url_allowed, try_client, try_streaming_client};
use crate::session_portal::resolve_deslicer_api_url;
use crate::token_store::load_active_session;
use crate::Ctx;

use super::http_errors::map_error_body;
use super::ids::parse_run_id;
pub use super::types::{AgentSummary, RunListBody, RunListItem, RunOutput, RunStatus, StartedRun};

const LIST_PATH: &str = "api/cli/agents";
const RUN_PATH: &str = "api/cli/agents/runs";

/// Header the server echoes so a run can be correlated with its ledger row.
const RUN_ID_HEADER: &str = "x-deslicer-run-id";
const CONVERSATION_ID_HEADER: &str = "x-deslicer-conversation-id";

#[derive(Debug, Deserialize)]
struct AgentListResponse {
    #[serde(default)]
    agents: Vec<AgentSummary>,
}

#[derive(Debug, Serialize)]
struct RunRequestBody<'a> {
    #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
    agent_id: Option<&'a str>,
    prompt: &'a str,
    #[serde(rename = "conversationId", skip_serializing_if = "Option::is_none")]
    conversation_id: Option<&'a str>,
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
            base: resolve_deslicer_api_url(ctx, &session),
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
    ///
    /// `agent_id` is omitted to run the tenant Orchestrator.
    pub async fn start_run(
        &self,
        agent_id: Option<&str>,
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

    pub async fn list_runs(
        &self,
        limit: Option<u32>,
        status: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<RunListBody, CliError> {
        let mut url = self.endpoint(RUN_PATH)?;
        {
            let mut pairs = url.query_pairs_mut();
            if let Some(limit) = limit {
                pairs.append_pair("limit", &limit.to_string());
            }
            if let Some(status) = status {
                pairs.append_pair("status", status);
            }
            if let Some(cursor) = cursor {
                pairs.append_pair("cursor", cursor);
            }
        }
        let response = self
            .json
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("list runs: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode run list: {e}")))
    }

    pub async fn latest_run(&self) -> Result<RunListItem, CliError> {
        self.try_latest_run().await?.ok_or_else(|| {
            CliError::Other(
                "no runs yet. Start one with `deslicer agent` or `deslicer agent run`.".into(),
            )
        })
    }

    /// Latest run for this session, or `None` when this member has none.
    ///
    /// Every 404 from `/latest` is "no row" — the handler uses the same
    /// isolation 404 as a missing id, so a leaked handle cannot be told
    /// apart from an empty history.
    pub async fn try_latest_run(&self) -> Result<Option<RunListItem>, CliError> {
        let url = self.endpoint(&format!("{RUN_PATH}/latest"))?;
        let response = self
            .json
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("read latest run: {e}")))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode latest run: {e}")))
            .map(Some)
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

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
