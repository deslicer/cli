use crate::errors::CliError;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePlan {
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanProgress {
    pub id: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ReconcileMode {
    PlanOnly,
    Apply,
}

impl ReconcileMode {
    fn as_str(self) -> &'static str {
        match self {
            ReconcileMode::PlanOnly => "plan-only",
            ReconcileMode::Apply => "apply",
        }
    }
}

pub struct Client {
    base: url::Url,
    token: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base: url::Url, token: String) -> Self {
        Self {
            base,
            token,
            http: reqwest::Client::new(),
        }
    }

    pub async fn reconcile(
        &self,
        environment: &Option<String>,
        mode: ReconcileMode,
    ) -> Result<ChangePlan, CliError> {
        #[derive(Serialize)]
        struct ReconcileBody<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            environment: Option<&'a str>,
            mode: &'static str,
        }

        let body = ReconcileBody {
            environment: environment.as_deref(),
            mode: mode.as_str(),
        };
        self.post_json("api/v1/state/reconcile", &body).await
    }

    pub async fn get_plan(&self, plan_id: &str) -> Result<ChangePlan, CliError> {
        let path = format!("api/v1/plans/{plan_id}");
        self.get_json(&path).await
    }

    pub async fn list_plans(&self, environment: Option<&str>) -> Result<Vec<ChangePlan>, CliError> {
        let mut path = "api/v1/plans".to_string();
        if let Some(env) = environment {
            let encoded =
                url::form_urlencoded::byte_serialize(env.as_bytes()).collect::<String>();
            path.push_str(&format!("?environment={encoded}"));
        }
        self.get_plans(&path).await
    }

    pub async fn approve(&self, plan_id: &str) -> Result<ChangePlan, CliError> {
        let path = format!("api/v1/plans/{plan_id}/approve");
        self.post_json_empty(&path).await
    }

    pub async fn reject(&self, plan_id: &str) -> Result<ChangePlan, CliError> {
        let path = format!("api/v1/plans/{plan_id}/reject");
        self.post_json_empty(&path).await
    }

    pub async fn execute(&self, plan_id: &str) -> Result<ChangePlan, CliError> {
        let path = format!("api/v1/plans/{plan_id}/execute");
        self.post_json_empty(&path).await
    }

    pub async fn progress(&self, plan_id: &str) -> Result<PlanProgress, CliError> {
        let path = format!("api/v1/plans/{plan_id}/progress");
        self.get_json(&path).await
    }

    async fn post_json_empty(&self, path: &str) -> Result<ChangePlan, CliError> {
        self.request_json(Method::POST, path, None::<&()>).await
    }

    async fn post_json<T: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<ChangePlan, CliError> {
        self.request_json(Method::POST, path, Some(body)).await
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, CliError> {
        self.request_json(Method::GET, path, None::<&()>).await
    }

    async fn get_plans(&self, path: &str) -> Result<Vec<ChangePlan>, CliError> {
        let bytes = self.request_bytes(Method::GET, path, None::<&()>).await?;
        if let Ok(plans) = serde_json::from_slice::<Vec<ChangePlan>>(&bytes) {
            return Ok(plans);
        }
        #[derive(Deserialize)]
        struct PlansWrapper {
            plans: Vec<ChangePlan>,
        }
        serde_json::from_slice::<PlansWrapper>(&bytes)
            .map(|w| w.plans)
            .map_err(|e| CliError::Transport(format!("invalid plans JSON: {e}")))
    }

    async fn request_json<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, CliError>
    where
        T: for<'de> Deserialize<'de>,
        B: Serialize + ?Sized,
    {
        let bytes = self.request_bytes(method, path, body).await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("invalid JSON response: {e}")))
    }

    async fn request_bytes<B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<Vec<u8>, CliError>
    where
        B: Serialize + ?Sized,
    {
        const MAX_ATTEMPTS: u32 = 3;
        const BACKOFF_BASE_MS: u64 = 500;

        let url = join_api_path(&self.base, path)?;
        let mut attempt = 0u32;

        loop {
            attempt += 1;
            let mut req = self
                .http
                .request(method.clone(), url.clone())
                .header("Authorization", format!("Bearer {}", self.token));
            if let Some(payload) = body {
                req = req.json(payload);
            }

            let response = req
                .send()
                .await
                .map_err(|e| CliError::Transport(e.to_string()))?;

            let status = response.status();
            if (status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS)
                && attempt < MAX_ATTEMPTS
            {
                let delay = retry_delay(response.headers(), attempt, BACKOFF_BASE_MS);
                tokio::time::sleep(delay).await;
                continue;
            }

            let retry_after = parse_retry_after_header(response.headers());
            let bytes = response
                .bytes()
                .await
                .map_err(|e| CliError::Transport(e.to_string()))?;

            if status.is_success() {
                return Ok(bytes.to_vec());
            }

            let body_text = String::from_utf8_lossy(&bytes).into_owned();
            return Err(map_observer_error(status, &body_text, retry_after));
        }
    }
}

fn join_api_path(base: &url::Url, path: &str) -> Result<url::Url, CliError> {
    base.join(path)
        .map_err(|e| CliError::Transport(format!("invalid URL join: {e}")))
}

fn retry_delay(headers: &reqwest::header::HeaderMap, attempt: u32, base_ms: u64) -> Duration {
    if let Some(secs) = parse_retry_after_header(headers) {
        return Duration::from_secs(secs);
    }
    let multiplier = 1u64.checked_shl(attempt.saturating_sub(1)).unwrap_or(1);
    Duration::from_millis(base_ms.saturating_mul(multiplier))
}

fn map_observer_error(
    status: reqwest::StatusCode,
    body: &str,
    retry_after_secs: Option<u64>,
) -> CliError {
    let message = error_message(body, status);
    match status.as_u16() {
        400 => CliError::UnsupportedPlatform(message),
        401 => CliError::OidcRejected(message),
        403 => {
            if mentions_environment(body) {
                CliError::EnvironmentNotBound(message)
            } else {
                CliError::RepoNotAllowlisted(message)
            }
        }
        404 => CliError::PlanNotFound(message),
        409 => CliError::AmbiguousBinding(message),
        429 => CliError::RateLimited {
            retry_after_secs: retry_after_secs.unwrap_or(30),
        },
        500..=599 => CliError::BackendUnavailable(status.to_string()),
        _ => CliError::Other(message),
    }
}

fn error_message(body: &str, status: reqwest::StatusCode) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        for key in ["detail", "error", "message"] {
            if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    return text.to_string();
                }
            }
        }
    }
    if body.trim().is_empty() {
        format!("HTTP {}", status)
    } else {
        body.trim().to_string()
    }
}

fn mentions_environment(text: &str) -> bool {
    text.to_ascii_lowercase().contains("environment")
}

fn parse_retry_after_header(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}
