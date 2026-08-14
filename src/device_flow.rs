use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::json;

use crate::errors::CliError;
use crate::token_store::StoredSession;
use crate::Ctx;

const POLL_TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Debug, Deserialize)]
struct StartResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: u64,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct TokenSuccess {
    cli_session_token: String,
    expires_at: String,
    tenant_id: String,
    display_name: String,
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

    eprintln!(
        "Open {} and enter code: {}",
        started.verification_uri, started.user_code
    );

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

fn join_api(base: &url::Url, path: &str) -> Result<url::Url, CliError> {
    base.join(path)
        .map_err(|e| CliError::Transport(format!("invalid URL join: {e}")))
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
        assert!(session.observer_api_url.ends_with("/api/cli/observer/"));
        assert!(session.cli_session_token.starts_with("dslcli_"));
    }
}
