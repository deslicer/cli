use serde::{Deserialize, Serialize};

use crate::device_flow::join_api;
use crate::errors::CliError;
use crate::http::try_client;
use crate::session_portal::resolve_deslicer_api_url;
use crate::token_store::load_active_session;
use crate::Ctx;

use crate::commands::agent::http_errors::map_error_body;

const ROUTINES_PATH: &str = "api/cli/routines";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineSummary {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineRun {
    pub id: String,
    #[serde(default)]
    pub routine_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineReport {
    pub run_id: String,
    #[serde(default)]
    pub routine_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub report: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ListRoutinesResponse {
    #[serde(default)]
    routines: Vec<RoutineSummary>,
}

#[derive(Deserialize)]
struct ListRunsResponse {
    #[serde(default)]
    runs: Vec<RoutineRun>,
}

pub struct RoutineClient {
    base: url::Url,
    token: String,
    http: reqwest::Client,
}

impl RoutineClient {
    pub fn from_ctx(ctx: &Ctx) -> Result<Self, CliError> {
        let session = load_active_session()?.ok_or_else(|| {
            CliError::Other(
                "no active device session. Run `deslicer auth login` first \
                 (routines are not available with a static API token)."
                    .into(),
            )
        })?;
        Ok(Self {
            base: resolve_deslicer_api_url(ctx, &session),
            token: session.cli_session_token,
            http: try_client()?,
        })
    }

    pub async fn list(&self) -> Result<Vec<RoutineSummary>, CliError> {
        let url = self.endpoint(ROUTINES_PATH)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("list routines: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| CliError::Transport(format!("list routines body: {e}")))?;
        if let Ok(list) = serde_json::from_slice::<Vec<RoutineSummary>>(&bytes) {
            return Ok(list);
        }
        let wrapped: ListRoutinesResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("decode routines: {e}")))?;
        Ok(wrapped.routines)
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<RoutineSummary, CliError> {
        let action = if enabled { "enable" } else { "disable" };
        let path = format!("{ROUTINES_PATH}/{id}/{action}");
        let url = self.endpoint(&path)?;
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("{action} routine: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode routine: {e}")))
    }

    pub async fn run(&self, id: &str, model: Option<&str>) -> Result<RoutineRun, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            model: Option<&'a str>,
        }
        let path = format!("{ROUTINES_PATH}/{id}/run");
        let url = self.endpoint(&path)?;
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.token)
            .json(&Body { model })
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("run routine: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode routine run: {e}")))
    }

    pub async fn list_runs(&self, id: &str) -> Result<Vec<RoutineRun>, CliError> {
        let path = format!("{ROUTINES_PATH}/{id}/runs");
        let url = self.endpoint(&path)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("list routine runs: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| CliError::Transport(format!("list runs body: {e}")))?;
        if let Ok(list) = serde_json::from_slice::<Vec<RoutineRun>>(&bytes) {
            return Ok(list);
        }
        let wrapped: ListRunsResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("decode routine runs: {e}")))?;
        Ok(wrapped.runs)
    }

    pub async fn report(&self, run_id: &str) -> Result<RoutineReport, CliError> {
        let path = format!("{ROUTINES_PATH}/runs/{run_id}/report");
        let url = self.endpoint(&path)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("routine report: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| CliError::Transport(format!("report body: {e}")))?;
        if let Ok(report) = serde_json::from_slice::<RoutineReport>(&bytes) {
            return Ok(report);
        }
        // Tolerate a bare JSON document as the report payload.
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("decode routine report: {e}")))?;
        Ok(RoutineReport {
            run_id: run_id.to_string(),
            routine_id: None,
            status: value
                .get("status")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            summary: value
                .get("summary")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            report: Some(value),
        })
    }

    fn endpoint(&self, path: &str) -> Result<url::Url, CliError> {
        join_api(&self.base, path)
    }

    async fn error_from(&self, response: reqwest::Response) -> CliError {
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60);
        let body = response.text().await.unwrap_or_default();
        map_error_body(status, &body, retry_after)
    }
}
