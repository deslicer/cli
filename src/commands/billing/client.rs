use serde::{Deserialize, Serialize};

use crate::commands::agent::http_errors::map_error_body;
use crate::device_flow::join_api;
use crate::errors::CliError;
use crate::http::try_client;
use crate::session_portal::resolve_deslicer_api_url;
use crate::token_store::load_active_session;
use crate::Ctx;

const BILLING_PATH: &str = "api/cli/billing";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingUsage {
    #[serde(default)]
    pub period_start: Option<String>,
    #[serde(default)]
    pub period_end: Option<String>,
    #[serde(default)]
    pub units: Option<f64>,
    #[serde(default)]
    pub unit_label: Option<String>,
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub breakdown: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingCredits {
    #[serde(default)]
    pub balance: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

pub struct BillingClient {
    base: url::Url,
    token: String,
    http: reqwest::Client,
}

impl BillingClient {
    pub fn from_ctx(ctx: &Ctx) -> Result<Self, CliError> {
        let session = load_active_session()?.ok_or_else(|| {
            CliError::Other(
                "no active device session. Run `deslicer auth login` first \
                 (billing is not available with a static API token)."
                    .into(),
            )
        })?;
        Ok(Self {
            base: resolve_deslicer_api_url(ctx, &session),
            token: session.cli_session_token,
            http: try_client()?,
        })
    }

    pub async fn usage(&self) -> Result<BillingUsage, CliError> {
        self.get_json(&format!("{BILLING_PATH}/usage"), "billing usage")
            .await
    }

    pub async fn credits(&self) -> Result<BillingCredits, CliError> {
        self.get_json(&format!("{BILLING_PATH}/credits"), "billing credits")
            .await
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> Result<T, CliError> {
        let url = join_api(&self.base, path)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("{what}: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            let body = response.text().await.unwrap_or_default();
            return Err(map_error_body(status, &body, retry_after));
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode {what}: {e}")))
    }
}
