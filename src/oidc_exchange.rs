use crate::ci::CiPlatform;
use crate::errors::CliError;
use serde::Serialize;

#[derive(Serialize)]
struct CiOidcRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<&'a str>,
}

#[derive(serde::Deserialize)]
struct CiOidcResponse {
    access_token: String,
}

pub async fn exchange(
    observer_api_url: &url::Url,
    jwt: &str,
    platform: CiPlatform,
    environment: Option<&str>,
) -> Result<String, CliError> {
    let url = observer_api_url
        .join("api/v1/auth/ci-oidc")
        .map_err(|e| CliError::Transport(format!("invalid URL join: {e}")))?;

    let body = match environment {
        Some(env) => CiOidcRequest {
            environment: Some(env),
        },
        None => CiOidcRequest { environment: None },
    };

    let http = reqwest::Client::new();
    let response = http
        .post(url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("X-Deslicer-CI-Platform", platform.header_value())
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| CliError::Transport(e.to_string()))?;

    let status = response.status();
    let retry_after = parse_retry_after_header(response.headers());
    let response_body = response
        .text()
        .await
        .map_err(|e| CliError::Transport(e.to_string()))?;

    if status.is_success() {
        let parsed: CiOidcResponse = serde_json::from_str(&response_body)
            .map_err(|e| CliError::Transport(format!("invalid ci-oidc JSON: {e}")))?;
        return Ok(parsed.access_token);
    }

    Err(map_exchange_error(status, &response_body, retry_after))
}

fn map_exchange_error(
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
