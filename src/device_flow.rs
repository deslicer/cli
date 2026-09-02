use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::json;

use crate::environment_name::is_valid_environment_name;
use crate::errors::CliError;
use crate::token_store::StoredSession;
use crate::Ctx;

const POLL_TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Debug, Deserialize)]
struct StartResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    interval: u64,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct TokenSuccess {
    cli_session_token: String,
    expires_at: String,
    tenant_id: String,
    display_name: String,
    /// Present on current DAI; old portals omit the field.
    #[serde(default)]
    tenant_slug: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenError {
    error: String,
    interval: Option<u64>,
}

pub async fn login_device_session(ctx: &Ctx) -> Result<StoredSession, CliError> {
    let start_url = join_api(&ctx.deslicer_api_url, "api/cli/device/start")?;
    crate::http::assert_url_allowed(&start_url)?;
    let http = crate::http::client();
    let start_res = http
        .post(start_url)
        .json(&json!({
            "client": "dap-cli",
            "version": env!("CARGO_PKG_VERSION"),
        }))
        .send()
        .await
        .map_err(|e| CliError::Transport(e.to_string()))?;
    if !start_res.status().is_success() {
        return Err(CliError::Other(format!(
            "device start failed: HTTP {}",
            start_res.status()
        )));
    }
    let started: StartResponse = start_res
        .json()
        .await
        .map_err(|e| CliError::Transport(format!("invalid device start JSON: {e}")))?;

    if let Some(complete) = &started.verification_uri_complete {
        eprintln!("Open {complete} to approve this CLI.");
        eprintln!(
            "Or enter code {} at {}",
            started.user_code, started.verification_uri
        );
    } else {
        eprintln!(
            "Open {} and enter code: {}",
            started.verification_uri, started.user_code
        );
    }

    poll_for_token(ctx, &started).await
}

async fn poll_for_token(ctx: &Ctx, started: &StartResponse) -> Result<StoredSession, CliError> {
    let token_url = join_api(&ctx.deslicer_api_url, "api/cli/device/token")?;
    crate::http::assert_url_allowed(&token_url)?;
    let http = crate::http::client();
    let deadline =
        Instant::now() + Duration::from_secs(started.expires_in.min(POLL_TIMEOUT.as_secs()));
    let mut interval = Duration::from_secs(started.interval.max(1));

    while Instant::now() < deadline {
        tokio::time::sleep(interval).await;
        let response = http
            .post(token_url.clone())
            .json(&json!({ "device_code": started.device_code }))
            .send()
            .await
            .map_err(|e| CliError::Transport(e.to_string()))?;
        if response.status().is_success() {
            let body: TokenSuccess = response
                .json()
                .await
                .map_err(|e| CliError::Transport(format!("invalid device token JSON: {e}")))?;
            let observer_api_url =
                join_api(&ctx.deslicer_api_url, "api/cli/observer/")?.to_string();
            return Ok(StoredSession {
                cli_session_token: body.cli_session_token,
                expires_at: body.expires_at,
                tenant_id: body.tenant_id,
                display_name: body.display_name,
                observer_api_url,
                tenant_slug: persistable_tenant_slug(body.tenant_slug.as_deref()),
                deslicer_api_url: Some(ctx.deslicer_api_url.to_string()),
            });
        }
        let err_body: TokenError = response.json().await.unwrap_or_else(|_| TokenError {
            error: "invalid_grant".into(),
            interval: None,
        });
        match err_body.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => {
                interval = Duration::from_secs(err_body.interval.unwrap_or(interval.as_secs() + 5));
            }
            "expired_token" => {
                return Err(CliError::Other(
                    "device code expired; run `deslicer auth login` again".into(),
                ));
            }
            "access_denied" => {
                return Err(CliError::Other(
                    "device login was denied in the portal".into(),
                ));
            }
            other => {
                return Err(CliError::Other(format!("device token failed: {other}")));
            }
        }
    }
    Err(CliError::Other(
        "timed out waiting for portal approval".into(),
    ))
}

pub(crate) fn join_api(base: &url::Url, path: &str) -> Result<url::Url, CliError> {
    base.join(path)
        .map_err(|e| CliError::Transport(format!("invalid URL join: {e}")))
}

/// Persist only a usable Observer env name. UUID / missing / garbage → None
/// so init/inventory keep requiring `--environment` against old DAI.
fn persistable_tenant_slug(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() || looks_like_uuid(trimmed) {
        return None;
    }
    if !is_valid_environment_name(trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

fn looks_like_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes[8] == b'-'
        && bytes[13] == b'-'
        && bytes[18] == b'-'
        && bytes[23] == b'-'
        && value.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::LogFormat;
    use serde_json::json;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn login_polls_until_session_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/cli/device/start"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code": "devcode1234567890abcd",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://app.deslicer.ai/dashboard/cli-auth",
                "verification_uri_complete": "https://app.deslicer.ai/dashboard/cli-auth?user_code=ABCD-EFGH",
                "interval": 1,
                "expires_in": 60
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/cli/device/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "cli_session_token": format!("dslcli_{}", "ab".repeat(32)),
                "expires_at": "2099-01-01T00:00:00.000Z",
                "tenant_id": "tenant-1",
                "display_name": "Ada"
            })))
            .mount(&server)
            .await;

        let ctx = Ctx {
            deslicer_api_url: Url::parse(&format!("{}/", server.uri())).unwrap(),
            observer_api_url: None,
            ci_override: None,
            log_format: LogFormat::Human,
        };
        let session = login_device_session(&ctx).await.unwrap();
        assert_eq!(session.display_name, "Ada");
        assert_eq!(session.tenant_id, "tenant-1");
        assert_eq!(session.tenant_slug, None);
        assert!(session.observer_api_url.ends_with("/api/cli/observer/"));
        assert_eq!(
            session.deslicer_api_url.as_deref(),
            Some(ctx.deslicer_api_url.as_str())
        );
        assert!(session.cli_session_token.starts_with("dslcli_"));
    }

    #[test]
    fn token_success_deserializes_optional_tenant_slug() {
        let with_slug: TokenSuccess = serde_json::from_value(json!({
            "cli_session_token": "dslcli_ab",
            "expires_at": "2099-01-01T00:00:00.000Z",
            "tenant_id": "2fb5ef22-12ad-4d20-9e0f-4736f47953bb",
            "display_name": "Ada",
            "tenant_slug": "dap-102"
        }))
        .unwrap();
        assert_eq!(
            persistable_tenant_slug(with_slug.tenant_slug.as_deref()).as_deref(),
            Some("dap-102")
        );

        let omitted: TokenSuccess = serde_json::from_value(json!({
            "cli_session_token": "dslcli_ab",
            "expires_at": "2099-01-01T00:00:00.000Z",
            "tenant_id": "2fb5ef22-12ad-4d20-9e0f-4736f47953bb",
            "display_name": "Ada"
        }))
        .unwrap();
        assert_eq!(omitted.tenant_slug, None);

        let uuid_slug: TokenSuccess = serde_json::from_value(json!({
            "cli_session_token": "dslcli_ab",
            "expires_at": "2099-01-01T00:00:00.000Z",
            "tenant_id": "2fb5ef22-12ad-4d20-9e0f-4736f47953bb",
            "display_name": "Ada",
            "tenant_slug": "2fb5ef22-12ad-4d20-9e0f-4736f47953bb"
        }))
        .unwrap();
        assert_eq!(
            persistable_tenant_slug(uuid_slug.tenant_slug.as_deref()),
            None
        );
    }

    #[tokio::test]
    async fn login_persists_tenant_slug_from_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/cli/device/start"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code": "devcode1234567890abcd",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://app.deslicer.ai/dashboard/cli-auth",
                "interval": 1,
                "expires_in": 60
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/cli/device/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "cli_session_token": format!("dslcli_{}", "cd".repeat(32)),
                "expires_at": "2099-01-01T00:00:00.000Z",
                "tenant_id": "2fb5ef22-12ad-4d20-9e0f-4736f47953bb",
                "display_name": "Ada",
                "tenant_slug": "dap-102"
            })))
            .mount(&server)
            .await;

        let ctx = Ctx {
            deslicer_api_url: Url::parse(&format!("{}/", server.uri())).unwrap(),
            observer_api_url: None,
            ci_override: None,
            log_format: LogFormat::Human,
        };
        let session = login_device_session(&ctx).await.unwrap();
        assert_eq!(session.tenant_slug.as_deref(), Some("dap-102"));
        assert_eq!(session.tenant_id, "2fb5ef22-12ad-4d20-9e0f-4736f47953bb");
        assert_eq!(
            session.deslicer_api_url.as_deref(),
            Some(ctx.deslicer_api_url.as_str())
        );
    }
}
