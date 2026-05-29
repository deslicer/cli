use crate::ci::{AUDIENCE, CiPlatform};
use crate::errors::CliError;
use crate::Ctx;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ResolvedBackend {
    pub observer_api_url: url::Url,
    pub audience: String,
    pub resolution_path: String,
}

#[derive(Serialize)]
struct ResolveBackendRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_id: Option<&'a str>,
}

#[derive(serde::Deserialize)]
struct ResolveBackendResponse {
    observer_api_url: String,
    audience: String,
    resolution_path: String,
}

pub async fn resolve(
    ctx: &Ctx,
    jwt: &str,
    platform: CiPlatform,
    environment: Option<&str>,
    plan_id: Option<&str>,
) -> Result<ResolvedBackend, CliError> {
    if let Some(url) = ctx.observer_api_url.clone() {
        return Ok(ResolvedBackend {
            observer_api_url: url,
            audience: AUDIENCE.to_string(),
            resolution_path: "observer_url_override".to_string(),
        });
    }

    let url = join_api_path(&ctx.deslicer_api_url, "api/cli/resolve-backend")?;
    let body = ResolveBackendRequest {
        repo: repo_from_ci(platform),
        environment,
        plan_id,
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
        let parsed: ResolveBackendResponse = serde_json::from_str(&response_body)
            .map_err(|e| CliError::Transport(format!("invalid resolve-backend JSON: {e}")))?;
        let observer_api_url = url::Url::parse(&parsed.observer_api_url).map_err(|e| {
            CliError::Transport(format!("invalid observer_api_url in response: {e}"))
        })?;
        return Ok(ResolvedBackend {
            observer_api_url,
            audience: parsed.audience,
            resolution_path: parsed.resolution_path,
        });
    }

    Err(map_resolver_error(status, &response_body, retry_after))
}

fn repo_from_ci(platform: CiPlatform) -> Option<String> {
    let key = match platform {
        CiPlatform::Github => "GITHUB_REPOSITORY",
        CiPlatform::Gitlab => "CI_PROJECT_PATH",
        CiPlatform::Azure => "BUILD_REPOSITORY_NAME",
        CiPlatform::Bitbucket => "BITBUCKET_REPO_FULL_NAME",
        CiPlatform::Local => return None,
    };
    std::env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn join_api_path(base: &url::Url, path: &str) -> Result<url::Url, CliError> {
    base.join(path)
        .map_err(|e| CliError::Transport(format!("invalid URL join: {e}")))
}

fn map_resolver_error(
    status: reqwest::StatusCode,
    body: &str,
    retry_after_secs: Option<u64>,
) -> CliError {
    let message = error_message(body, status);
    match status.as_u16() {
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
