//! HTTP status → `CliError` mapping and retry helpers for Observer requests.

use std::time::Duration;

use crate::errors::CliError;

pub(crate) fn map_observer_error(
    status: reqwest::StatusCode,
    body: &str,
    retry_after_secs: Option<u64>,
) -> CliError {
    let message = error_message(body, status);
    match status.as_u16() {
        400 => CliError::UnsupportedPlatform(message),
        401 => CliError::OidcRejected(message),
        403 => {
            if mentions_human_approval(body) {
                CliError::HumanApprovalRequired(message)
            } else if mentions_worker_plane(body) {
                CliError::Other(worker_plane_message(&message))
            } else if mentions_environment(body) {
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

pub(crate) fn error_message(body: &str, status: reqwest::StatusCode) -> String {
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

/// Observer returns `mfa_required` when approval lacks a human identity; the
/// CI proxy returns `approval_not_found` / `self_approval_blocked` when no
/// valid GitHub Environment reviewer could be attested.
fn mentions_human_approval(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    ["mfa", "approval_not_found", "self_approval", "reviewer"]
        .iter()
        .any(|needle| lowered.contains(needle))
}

fn mentions_worker_plane(text: &str) -> bool {
    text.to_ascii_lowercase().contains("worker_plane")
}

fn worker_plane_message(server: &str) -> String {
    if server.to_ascii_lowercase().contains("worker plane") {
        format!(
            "{server} Enable the worker plane in the portal before creating a bootstrap enrollment token."
        )
    } else {
        "Worker plane is not enabled for this tenant. Enable it in the portal before creating a bootstrap enrollment token.".into()
    }
}

pub(crate) fn retry_delay(
    headers: &reqwest::header::HeaderMap,
    attempt: u32,
    base_ms: u64,
) -> Duration {
    if let Some(secs) = parse_retry_after_header(headers) {
        return Duration::from_secs(secs);
    }
    let multiplier = 1u64.checked_shl(attempt.saturating_sub(1)).unwrap_or(1);
    Duration::from_millis(base_ms.saturating_mul(multiplier))
}

pub(crate) fn parse_retry_after_header(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_plane_forbidden_is_not_repo_allowlist() {
        let err = map_observer_error(
            reqwest::StatusCode::FORBIDDEN,
            r#"{"error":"worker_plane_not_enabled"}"#,
            None,
        );
        match err {
            CliError::Other(message) => {
                assert!(message.to_ascii_lowercase().contains("worker plane"));
            }
            other => panic!("expected Other, got {other}"),
        }
    }
}
