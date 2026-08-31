//! Maps the deslicer-ai agent-run error contract onto `CliError`.

use serde::Deserialize;

use crate::errors::CliError;

#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// Maps the server's error contract onto a `CliError`, and therefore onto an
/// exit code. The `error` codes are a stable interface — see
/// `lib/integrations/cli-device/agent-run/errors.ts` in deslicer-ai.
pub fn map_error_body(status: reqwest::StatusCode, body: &str, retry_after_secs: u64) -> CliError {
    let parsed: Option<ErrorBody> = serde_json::from_str(body).ok();
    let code = parsed.as_ref().and_then(|b| b.error.as_deref());
    let message = parsed
        .as_ref()
        .and_then(|b| b.message.as_deref())
        .filter(|m| !m.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("agent request failed (HTTP {})", status.as_u16()));

    match code {
        Some("too_many_runs") => CliError::RateLimited { retry_after_secs },
        Some("unauthorized") => CliError::Other(format!(
            "{message}\nRun `deslicer auth login` to start a new device session."
        )),
        Some("agent_not_found") => CliError::Other(format!(
            "{message}\nRun `deslicer agent list` to see the agents you can run."
        )),
        Some("run_in_progress") | Some("run_already_completed") => CliError::Other(message),
        Some("run_not_found") => CliError::Other(format!(
            "{message}\nRun ids are printed when a run starts, and are scoped to the \
             account that started them."
        )),
        Some("run_failed") => CliError::AgentRunFailed(message),
        Some(_) => CliError::Other(message),
        // No parsable body: fall back to the status class. A 5xx or 429 from
        // an intermediate proxy never reaches the handler's error contract.
        None if status == reqwest::StatusCode::TOO_MANY_REQUESTS => {
            CliError::RateLimited { retry_after_secs }
        }
        None if status.is_server_error() => CliError::BackendUnavailable(status.to_string()),
        None => CliError::Other(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn maps_too_many_runs_to_rate_limited() {
        let err = map_error_body(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":"too_many_runs","message":"Too many runs"}"#,
            17,
        );
        assert!(matches!(
            err,
            CliError::RateLimited {
                retry_after_secs: 17
            }
        ));
        assert_eq!(err.exit_code(), 9);
    }

    #[test]
    fn maps_run_failed_to_its_own_exit_code() {
        let err = map_error_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"error":"run_failed","message":"The run could not be started."}"#,
            30,
        );
        assert!(matches!(err, CliError::AgentRunFailed(_)));
        assert_eq!(err.exit_code(), 13);
    }

    #[test]
    fn unauthorized_points_at_the_login_command() {
        let err = map_error_body(
            StatusCode::UNAUTHORIZED,
            r#"{"error":"unauthorized","message":"CLI session expired."}"#,
            30,
        );
        assert!(err.to_string().contains("deslicer auth login"));
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn agent_not_found_points_at_the_list_command() {
        let err = map_error_body(
            StatusCode::NOT_FOUND,
            r#"{"error":"agent_not_found","message":"No such agent."}"#,
            30,
        );
        assert!(err.to_string().contains("deslicer agent list"));
    }

    #[test]
    fn unknown_code_keeps_the_server_message() {
        let err = map_error_body(
            StatusCode::BAD_REQUEST,
            r#"{"error":"invalid_request","message":"prompt is required"}"#,
            30,
        );
        assert_eq!(err.to_string(), "prompt is required");
    }

    #[test]
    fn html_error_page_from_a_proxy_falls_back_to_the_status_class() {
        let err = map_error_body(
            StatusCode::BAD_GATEWAY,
            "<html><body>502 Bad Gateway</body></html>",
            30,
        );
        assert!(matches!(err, CliError::BackendUnavailable(_)));
        assert_eq!(err.exit_code(), 10);
    }

    #[test]
    fn proxy_429_without_a_body_is_still_rate_limited() {
        let err = map_error_body(StatusCode::TOO_MANY_REQUESTS, "", 5);
        assert!(matches!(
            err,
            CliError::RateLimited {
                retry_after_secs: 5
            }
        ));
    }

    #[test]
    fn blank_message_falls_back_to_the_status_code() {
        let err = map_error_body(
            StatusCode::FORBIDDEN,
            r#"{"error":"billing_blocked","message":"  "}"#,
            30,
        );
        assert!(err.to_string().contains("403"));
    }
}
