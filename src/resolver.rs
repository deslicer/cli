use crate::ci::{CiPlatform, AUDIENCE};
use crate::errors::CliError;
use crate::Ctx;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ResolvedBackend {
    pub observer_api_url: url::Url,
    pub audience: String,
    pub resolution_path: String,
    pub proxy_mode: bool,
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
    #[serde(default)]
    proxy_mode: bool,
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
            proxy_mode: false,
        });
    }

    let url = join_api_path(&ctx.deslicer_api_url, "api/cli/resolve-backend")?;
    crate::http::assert_url_allowed(&url)?;
    let body = ResolveBackendRequest {
        repo: repo_from_ci(platform),
        environment,
        plan_id,
    };

    let http = crate::http::client();
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
            proxy_mode: parsed.proxy_mode,
        });
    }

    Err(map_resolver_error(status, &response_body, retry_after))
}

#[derive(serde::Deserialize)]
struct ResolveEnvironmentsResponse {
    environments: Vec<String>,
}

/// Discover every environment bound to this CI repo/branch.
/// An empty list means the tenant has no named bindings; callers may fall
/// back to a single unscoped plan.
pub async fn resolve_environments(
    ctx: &Ctx,
    jwt: &str,
    platform: CiPlatform,
) -> Result<Vec<String>, CliError> {
    if platform == CiPlatform::Local {
        return Ok(Vec::new());
    }

    let url = join_api_path(&ctx.deslicer_api_url, "api/cli/resolve-environments")?;
    crate::http::assert_url_allowed(&url)?;

    let http = crate::http::client();
    let response = http
        .post(url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("X-Deslicer-CI-Platform", platform.header_value())
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({}))
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
        let parsed: ResolveEnvironmentsResponse = serde_json::from_str(&response_body)
            .map_err(|e| CliError::Transport(format!("invalid resolve-environments JSON: {e}")))?;
        return Ok(parsed.environments);
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
        format!("HTTP {status}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::LogFormat;
    use serde_json::json;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_ctx(base: &str) -> Ctx {
        Ctx {
            deslicer_api_url: Url::parse(base).unwrap(),
            observer_api_url: None,
            ci_override: Some(CiPlatform::Github),
            log_format: LogFormat::Human,
        }
    }

    #[tokio::test]
    async fn resolve_environments_returns_bound_names() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/cli/resolve-environments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "environments": ["staging", "prod"]
            })))
            .mount(&server)
            .await;

        let ctx = test_ctx(&format!("{}/", server.uri()));
        let names = resolve_environments(&ctx, "jwt", CiPlatform::Github)
            .await
            .unwrap();
        assert_eq!(names, vec!["staging".to_string(), "prod".to_string()]);
    }

    #[tokio::test]
    async fn resolve_environments_skips_local_platform() {
        let ctx = test_ctx("https://api.deslicer.ai/");
        let names = resolve_environments(&ctx, "jwt", CiPlatform::Local)
            .await
            .unwrap();
        assert!(names.is_empty());
    }
}
