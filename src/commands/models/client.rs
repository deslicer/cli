use serde::{Deserialize, Serialize};

use crate::commands::agent::http_errors::map_error_body;
use crate::device_flow::join_api;
use crate::errors::CliError;
use crate::http::try_client;
use crate::session_portal::resolve_deslicer_api_url;
use crate::token_store::load_active_session;
use crate::Ctx;

const MODELS_PATH: &str = "api/cli/models";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSummary {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub byok: Option<bool>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ByokStatus {
    #[serde(default)]
    pub configured: Option<bool>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub key_hint: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ByokSetResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub status: Option<ByokStatus>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Deserialize)]
struct ListModelsResponse {
    #[serde(default)]
    models: Vec<ModelSummary>,
}

pub struct ModelsClient {
    base: url::Url,
    token: String,
    http: reqwest::Client,
}

impl ModelsClient {
    pub fn from_ctx(ctx: &Ctx) -> Result<Self, CliError> {
        let session = load_active_session()?.ok_or_else(|| {
            CliError::DeviceSessionRequired(
                "no active device session. Run `deslicer auth login` first \
                 (models are not available with a static API token)."
                    .into(),
            )
        })?;
        Ok(Self {
            base: resolve_deslicer_api_url(ctx, &session),
            token: session.cli_session_token,
            http: try_client()?,
        })
    }

    pub async fn list(&self) -> Result<Vec<ModelSummary>, CliError> {
        let url = self.endpoint(MODELS_PATH)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("list models: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| CliError::Transport(format!("list models body: {e}")))?;
        if let Ok(list) = serde_json::from_slice::<Vec<ModelSummary>>(&bytes) {
            return Ok(list);
        }
        let wrapped: ListModelsResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("decode models: {e}")))?;
        Ok(wrapped.models)
    }

    pub async fn byok_status(&self) -> Result<ByokStatus, CliError> {
        let url = self.endpoint(&format!("{MODELS_PATH}/byok"))?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("byok status: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode byok status: {e}")))
    }

    pub async fn byok_set(&self, provider: &str, key: &str) -> Result<ByokSetResult, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            provider: &'a str,
            key: &'a str,
        }
        let url = self.endpoint(&format!("{MODELS_PATH}/byok"))?;
        let response = self
            .http
            .put(url)
            .bearer_auth(&self.token)
            .json(&Body { provider, key })
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("byok set: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode byok set: {e}")))
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
