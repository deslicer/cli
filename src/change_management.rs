//! Tenant change-management mode helpers (direct vs git-backed).

use crate::commands::update::BINARY_NAME;
use crate::errors::CliError;

pub const MODE_GIT_BACKED: &str = "git_backed";

/// Observer `POST /api/v1/plans` conflict when direct/bundle plan creation is blocked.
pub const ERROR_GIT_BACKED_MODE_ENABLED: &str = "git_backed_mode_enabled";

pub fn is_git_backed_mode_error(body: &str) -> bool {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        for key in ["error", "detail", "message"] {
            if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
                if text == ERROR_GIT_BACKED_MODE_ENABLED
                    || text.to_ascii_lowercase().contains("git-backed change management")
                {
                    return true;
                }
            }
        }
    }
    body.to_ascii_lowercase().contains("git-backed change management")
}

pub fn git_backed_plan_guidance() -> String {
    format!(
        "Connect the GitHub App and bind this repository under Platform → CI environments, \
         then run `{name} change plan` from GitHub Actions with OIDC (omit the \
         OBSERVER_API_URL override). For edits outside CI, use the portal Propose change flow \
         (opens a pull request on the bound repository). See `{name} docs github-token` for \
         Observer-token CI limits.",
        name = BINARY_NAME,
    )
}

pub fn enrich_git_backed_plan_message(server_message: String) -> String {
    let guidance = git_backed_plan_guidance();
    if server_message.contains(&guidance) {
        return server_message;
    }
    format!("{server_message}\n{guidance}")
}

pub fn bundle_blocked_message() -> String {
    enrich_git_backed_plan_message(
        "This tenant uses git-backed change management. Bundle upload (Path B) is not \
         available — configuration changes must flow through the connected GitHub repository."
            .into(),
    )
}

pub fn map_http_conflict(body: &str, message: String, is_duplicate_plan: bool) -> CliError {
    if is_duplicate_plan {
        return CliError::Other(message);
    }
    if is_git_backed_mode_error(body) {
        return CliError::GitBackedChangeManagement(enrich_git_backed_plan_message(message));
    }
    CliError::AmbiguousBinding(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_git_backed_error_code() {
        let body = r#"{"error":"git_backed_mode_enabled","message":"blocked"}"#;
        assert!(is_git_backed_mode_error(body));
    }

    #[test]
    fn maps_conflict_to_git_backed_variant() {
        let body = r#"{"error":"git_backed_mode_enabled","message":"This tenant uses git-backed change management."}"#;
        let err = map_http_conflict(body, "This tenant uses git-backed change management.".into(), false);
        assert!(matches!(err, CliError::GitBackedChangeManagement(_)));
        assert_eq!(err.exit_code(), 7);
    }
}
