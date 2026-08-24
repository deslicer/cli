use serde::Deserialize;
use serde::Serialize;

use crate::device_flow::join_api;
use crate::errors::CliError;
use crate::token_store::StoredSession;
use crate::Ctx;

#[derive(Serialize)]
pub struct WorkerSnippetRequest {
    pub format: String,
    pub product: String,
    pub channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrollment_token: Option<String>,
}

#[derive(Deserialize)]
pub struct WorkerSnippetResponse {
    pub snippet: String,
}

pub async fn fetch_worker_install_snippet(
    ctx: &Ctx,
    session: &StoredSession,
    body: &WorkerSnippetRequest,
) -> Result<String, CliError> {
    let url = join_api(&ctx.deslicer_api_url, "api/cli/worker-install-snippet")?;
    crate::http::assert_url_allowed(&url)?;

    let response = crate::http::client()
        .post(url)
        .header(
            "Authorization",
            format!("Bearer {}", session.cli_session_token),
        )
        .json(body)
        .send()
        .await
        .map_err(|err| CliError::Transport(err.to_string()))?;

    let status = response.status();
    let retry_after = crate::observer_client::retry_after_header(response.headers());
    let bytes = response
        .bytes()
        .await
        .map_err(|err| CliError::Transport(err.to_string()))?;
    let text = String::from_utf8_lossy(&bytes).into_owned();

    if !status.is_success() {
        return Err(map_snippet_error(status, &text, retry_after));
    }

    let parsed: WorkerSnippetResponse = serde_json::from_str(&text)
        .map_err(|err| CliError::Transport(format!("invalid snippet JSON: {err}")))?;
    Ok(parsed.snippet)
}

fn map_snippet_error(
    status: reqwest::StatusCode,
    body: &str,
    retry_after_secs: Option<u64>,
) -> CliError {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return CliError::RateLimited {
            retry_after_secs: retry_after_secs.unwrap_or(30),
        };
    }
    if status.as_u16() == 409 {
        if let Some(message) = json_field(body, "message") {
            return CliError::Other(message);
        }
    }
    let message = crate::observer_client::error_message(body, status);
    if status.is_server_error() {
        return CliError::BackendUnavailable(status.to_string());
    }
    CliError::Other(message)
}

fn json_field(body: &str, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get(key)
                .and_then(|item| item.as_str())
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        })
}
