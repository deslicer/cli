//! HTTP status → `CliError` mapping and retry helpers for Observer requests.

use std::time::Duration;

use crate::errors::CliError;

/// How a 401 from Observer should be labelled for the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AuthRejectionKind {
    /// CI OIDC JWT, DAI resolve/exchange, or unspecified legacy clients.
    #[default]
    Oidc,
    /// Direct `OBSERVER_API_URL` + `DESLICER_API_TOKEN` path.
    ApiToken,
}

pub(crate) fn map_observer_error_with_auth(
    status: reqwest::StatusCode,
    body: &str,
    retry_after_secs: Option<u64>,
    auth: AuthRejectionKind,
    request_path: &str,
) -> CliError {
    let message = error_message(body, status);
    if let Some(error) = validation_error(body) {
        return error;
    }
    match status.as_u16() {
        400 => CliError::UnsupportedPlatform(message),
        401 => match auth {
            AuthRejectionKind::Oidc => CliError::OidcRejected(message),
            AuthRejectionKind::ApiToken => CliError::ApiTokenRejected(message),
        },
        403 => {
            if mentions_human_approval(body) {
                CliError::HumanApprovalRequired(message)
            } else if mentions_worker_plane(body) {
                CliError::Other(worker_plane_message(&message))
            } else if mentions_environment(body) {
                CliError::EnvironmentNotBound(message)
            } else if mentions_repo_not_allowlisted(body) {
                CliError::RepoNotAllowlisted(message)
            } else {
                CliError::Other(forbidden_message(body))
            }
        }
        404 => map_not_found(request_path, body, message),
        409 => crate::change_management::map_http_conflict(
            body,
            message,
            is_duplicate_plan_error(body),
        ),
        429 => CliError::RateLimited {
            retry_after_secs: retry_after_secs.unwrap_or(30),
        },
        500..=599 => CliError::BackendUnavailable(status.to_string()),
        _ => CliError::Other(message),
    }
}

/// Only a plan *lookup* 404 (GET/HEAD the plan resource itself) is "plan not
/// found". A generic 404 on catalog/upgrade/license/executions means the
/// Observer host does not expose that route — see issue #183.
fn map_not_found(request_path: &str, body: &str, message: String) -> CliError {
    if is_explicit_plan_not_found(body) || is_plan_lookup_path(request_path) {
        CliError::PlanNotFound(message)
    } else {
        CliError::EndpointNotAvailable(message)
    }
}

fn is_explicit_plan_not_found(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    ["error", "detail", "message"]
        .iter()
        .filter_map(|key| value.get(*key).and_then(|field| field.as_str()))
        .any(is_plan_absent_text)
}

/// Observer plan-lock helpers return `Plan <uuid> not found` (UUID between the
/// words), which does not contain the contiguous substring "plan not found".
fn is_plan_absent_text(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    let trimmed = lowered.trim();
    trimmed == "plan_not_found"
        || trimmed.contains("plan not found")
        || trimmed.contains("plan does not exist")
        || (trimmed.starts_with("plan ") && trimmed.ends_with(" not found"))
}

/// `api/v1/plans/{id}` or `v1/plans/{id}` — no further path segments.
fn is_plan_lookup_path(path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path).trim_matches('/');
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    matches!(
        parts.as_slice(),
        ["api", "v1", "plans", id] | ["v1", "plans", id] if !id.is_empty()
    )
}

fn validation_error(body: &str) -> Option<CliError> {
    let code = serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("error")?
        .as_str()?
        .to_string();
    match code.as_str() {
        "invalid_plan_id" => Some(CliError::InvalidInput("plan id must be a UUID".to_string())),
        "model_unavailable" => Some(CliError::ValidationModelUnavailable),
        "validation_timeout" => Some(CliError::ValidationTimeout),
        "validation_failed" | "validation_unavailable" => Some(CliError::ValidationUnavailable),
        _ => None,
    }
}

pub(crate) fn is_duplicate_plan_error(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .is_some_and(|code| code == "duplicate_plan")
}

pub(crate) fn error_message(body: &str, status: reqwest::StatusCode) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        for key in ["detail", "message", "error"] {
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

fn mentions_repo_not_allowlisted(body: &str) -> bool {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        return ["error", "detail", "message"]
            .iter()
            .filter_map(|key| value.get(key).and_then(|field| field.as_str()))
            .any(|text| {
                is_repo_not_allowlisted_code(text) || is_repo_not_allowlisted_message(text)
            });
    }
    is_repo_not_allowlisted_code(body) || is_repo_not_allowlisted_message(body)
}

fn is_repo_not_allowlisted_code(code: &str) -> bool {
    matches!(code, "repo_not_allowlisted" | "repository_not_allowlisted")
}

fn is_repo_not_allowlisted_message(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    [
        "repo not allowlisted",
        "repository not allowlisted",
        "repository is not allowlisted",
    ]
    .iter()
    .any(|phrase| lowered.contains(phrase))
}

fn forbidden_message(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return "observer request forbidden (HTTP 403)".to_string();
    };
    for key in ["error", "detail", "message"] {
        if let Some(text) = value.get(key).and_then(|field| field.as_str()) {
            if !text.is_empty() {
                return text.to_string();
            }
        }
    }
    "observer request forbidden (HTTP 403)".to_string()
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
    fn explicit_repo_allowlist_code_maps_to_repo_not_allowlisted() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::FORBIDDEN,
            r#"{"error":"repo_not_allowlisted"}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );

        assert!(matches!(err, CliError::RepoNotAllowlisted(_)));
        assert_eq!(err.exit_code(), 5);
    }

    #[test]
    fn explicit_repo_allowlist_message_maps_to_repo_not_allowlisted() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::FORBIDDEN,
            r#"{"error":"forbidden","message":"repository is not allowlisted"}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );

        assert!(matches!(err, CliError::RepoNotAllowlisted(_)));
    }

    #[test]
    fn identity_attestation_forbidden_preserves_stable_code() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::FORBIDDEN,
            r#"{"error":"identity_attestation_required","message":"attestation required"}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );

        match err {
            CliError::Other(message) => assert_eq!(message, "identity_attestation_required"),
            other => panic!("expected Other, got {other}"),
        }
    }

    #[test]
    fn compile_failed_forbidden_is_not_repo_allowlist() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::FORBIDDEN,
            r#"{"error":"compile_failed"}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );

        match err {
            CliError::Other(message) => assert_eq!(message, "compile_failed"),
            other => panic!("expected Other, got {other}"),
        }
    }

    #[test]
    fn worker_plane_forbidden_is_not_repo_allowlist() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::FORBIDDEN,
            r#"{"error":"worker_plane_not_enabled"}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );
        match err {
            CliError::Other(message) => {
                assert!(message.to_ascii_lowercase().contains("worker plane"));
            }
            other => panic!("expected Other, got {other}"),
        }
    }

    #[test]
    fn duplicate_plan_conflict_is_not_ambiguous_binding() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::CONFLICT,
            r#"{"error":"duplicate_plan","message":"An active plan already exists for this repository and commit SHA"}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );
        match err {
            CliError::Other(message) => {
                assert!(message.contains("active plan already exists"));
                assert!(!message.contains("duplicate_plan"));
            }
            other => panic!("expected Other, got {other}"),
        }
    }

    #[test]
    fn other_conflict_stays_ambiguous_binding() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::CONFLICT,
            r#"{"error":"ambiguous_environment","message":"multiple environments"}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );
        match err {
            CliError::AmbiguousBinding(message) => {
                assert!(message.contains("multiple environments"));
            }
            other => panic!("expected AmbiguousBinding, got {other}"),
        }
    }

    #[test]
    fn git_backed_mode_conflict_is_not_ambiguous_binding() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::CONFLICT,
            r#"{"error":"git_backed_mode_enabled","message":"This tenant uses git-backed change management."}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );
        match &err {
            CliError::GitBackedChangeManagement(message) => {
                assert!(message.contains("git-backed change management"));
                assert!(message.contains("GitHub App"));
            }
            other => panic!("expected GitBackedChangeManagement, got {other}"),
        }
        assert_eq!(err.exit_code(), 7);
    }

    #[test]
    fn validation_errors_keep_distinct_exit_contract() {
        let timeout = map_observer_error_with_auth(
            reqwest::StatusCode::GATEWAY_TIMEOUT,
            r#"{"error":"validation_timeout"}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );
        assert!(matches!(timeout, CliError::ValidationTimeout));

        let model = map_observer_error_with_auth(
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            r#"{"error":"model_unavailable"}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );
        assert!(matches!(model, CliError::ValidationModelUnavailable));
    }

    #[test]
    fn invalid_plan_id_is_a_usage_error() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":"invalid_plan_id"}"#,
            None,
            AuthRejectionKind::Oidc,
            "",
        );
        assert!(matches!(err, CliError::InvalidInput(_)));
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn unauthorized_defaults_to_oidc_rejection() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::UNAUTHORIZED,
            "",
            None,
            AuthRejectionKind::Oidc,
            "",
        );
        match &err {
            CliError::OidcRejected(message) => {
                assert!(message.contains("401"));
            }
            other => panic!("expected OidcRejected, got {other}"),
        }
        assert_eq!(err.exit_code(), 4);
        assert!(err.to_string().contains("OIDC token rejected"));
    }

    #[test]
    fn unauthorized_with_api_token_auth_is_not_oidc() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::UNAUTHORIZED,
            "",
            None,
            AuthRejectionKind::ApiToken,
            "",
        );
        match &err {
            CliError::ApiTokenRejected(message) => {
                assert!(message.contains("401"));
            }
            other => panic!("expected ApiTokenRejected, got {other}"),
        }
        assert_eq!(err.exit_code(), 4);
        let text = err.to_string();
        assert!(text.contains("API token rejected"));
        assert!(text.contains("DESLICER_API_TOKEN"));
        assert!(!text.contains("OIDC"));
    }

    #[test]
    fn plan_lookup_404_stays_plan_not_found() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::NOT_FOUND,
            r#"{"detail":"Not Found"}"#,
            None,
            AuthRejectionKind::Oidc,
            "api/v1/plans/missing-plan",
        );
        assert!(matches!(err, CliError::PlanNotFound(_)));
        assert_eq!(err.exit_code(), 11);
    }

    #[test]
    fn plan_subresource_404_is_endpoint_not_available() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::NOT_FOUND,
            r#"{"detail":"Not Found"}"#,
            None,
            AuthRejectionKind::Oidc,
            "api/v1/plans/done/executions",
        );
        assert!(matches!(err, CliError::EndpointNotAvailable(_)));
        assert_eq!(err.exit_code(), 15);
        assert!(err.to_string().contains("endpoint not available"));
    }

    #[test]
    fn catalog_404_is_endpoint_not_available() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::NOT_FOUND,
            r#"{"detail":"Not Found"}"#,
            None,
            AuthRejectionKind::ApiToken,
            "api/v1/catalog",
        );
        assert!(matches!(err, CliError::EndpointNotAvailable(_)));
        assert_eq!(err.exit_code(), 15);
    }

    #[test]
    fn explicit_plan_not_found_code_wins_on_any_path() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::NOT_FOUND,
            r#"{"error":"plan_not_found","message":"no such plan"}"#,
            None,
            AuthRejectionKind::Oidc,
            "api/v1/plans/x/diff",
        );
        assert!(matches!(err, CliError::PlanNotFound(_)));
        assert_eq!(err.exit_code(), 11);
    }

    #[test]
    fn approve_404_with_observer_plan_absent_message_is_plan_not_found() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::NOT_FOUND,
            r#"{"error":"not_found","message":"Plan 00000000-0000-0000-0000-000000000000 not found"}"#,
            None,
            AuthRejectionKind::Oidc,
            "api/v1/plans/00000000-0000-0000-0000-000000000000/approve",
        );
        assert!(matches!(err, CliError::PlanNotFound(_)), "got {err:?}");
        assert_eq!(err.exit_code(), 11);
        assert!(err.to_string().contains("plan not found"));
        assert!(!err.to_string().contains("endpoint not available"));
    }

    #[test]
    fn approve_404_without_plan_wording_stays_endpoint_not_available() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::NOT_FOUND,
            r#"{"error":"not_found","message":"Not Found"}"#,
            None,
            AuthRejectionKind::Oidc,
            "api/v1/plans/00000000-0000-0000-0000-000000000000/approve",
        );
        assert!(
            matches!(err, CliError::EndpointNotAvailable(_)),
            "got {err:?}"
        );
        assert_eq!(err.exit_code(), 15);
    }

    #[test]
    fn catalog_404_html_stays_endpoint_not_available() {
        let err = map_observer_error_with_auth(
            reqwest::StatusCode::NOT_FOUND,
            "<!DOCTYPE html><html><body>404</body></html>",
            None,
            AuthRejectionKind::ApiToken,
            "api/v1/catalog",
        );
        assert!(matches!(err, CliError::EndpointNotAvailable(_)));
        assert_eq!(err.exit_code(), 15);
    }

    #[test]
    fn plan_lookup_path_helper_rejects_subresources() {
        assert!(is_plan_lookup_path("api/v1/plans/abc"));
        assert!(is_plan_lookup_path("/v1/plans/abc?environment=show"));
        assert!(!is_plan_lookup_path("api/v1/plans/abc/executions"));
        assert!(!is_plan_lookup_path("api/v1/catalog"));
        assert!(!is_plan_lookup_path("api/v1/plans"));
    }

    #[test]
    fn plan_absent_text_matches_uuid_between_words() {
        assert!(is_plan_absent_text(
            "Plan 00000000-0000-0000-0000-000000000000 not found"
        ));
        assert!(is_plan_absent_text("plan not found"));
        assert!(is_plan_absent_text("plan_not_found"));
        assert!(!is_plan_absent_text("not_found"));
        assert!(!is_plan_absent_text("Not Found"));
    }
}
