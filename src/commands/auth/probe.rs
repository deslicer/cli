//! Shared Observer identity probe helpers for `auth whoami` / `auth login`.

use crate::errors::CliError;

/// True when Observer returned HTTP 403 for the identity probe.
///
/// Scope/forbid proves the bearer was authenticated; only 401 maps to
/// `ApiTokenRejected` / `OidcRejected` (not logged in, exit 4).
pub(crate) fn identity_probe_forbidden(err: &CliError) -> bool {
    match err {
        CliError::Other(msg) => {
            let lower = msg.to_ascii_lowercase();
            lower.contains("forbidden") || lower.contains("http 403")
        }
        // Other 403 classifiers from `map_observer_error_with_auth` still mean
        // the token was accepted before the forbid reason was applied.
        CliError::HumanApprovalRequired(_)
        | CliError::EnvironmentNotBound(_)
        | CliError::RepoNotAllowlisted(_) => true,
        _ => false,
    }
}

/// True when Observer does not expose `GET /me` (HTML 404 / missing route).
///
/// That is not a rejected token: treat as logged in, unverified (issue #246).
pub(crate) fn identity_probe_unavailable(err: &CliError) -> bool {
    matches!(err, CliError::EndpointNotAvailable(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_probe_forbidden_detects_403_not_401() {
        assert!(identity_probe_forbidden(&CliError::Other(
            "observer request forbidden (HTTP 403)".into()
        )));
        assert!(identity_probe_forbidden(&CliError::Other(
            "forbidden".into()
        )));
        assert!(!identity_probe_forbidden(&CliError::ApiTokenRejected(
            "HTTP 401 Unauthorized".into()
        )));
        assert!(!identity_probe_forbidden(&CliError::OidcRejected(
            "oidc_rejected".into()
        )));
    }

    #[test]
    fn identity_probe_unavailable_detects_endpoint_not_available() {
        assert!(identity_probe_unavailable(&CliError::EndpointNotAvailable(
            "GET api/v1/me → HTTP 404".into()
        )));
        assert!(!identity_probe_unavailable(&CliError::ApiTokenRejected(
            "HTTP 401 Unauthorized".into()
        )));
        assert!(!identity_probe_unavailable(&CliError::Other(
            "observer request forbidden (HTTP 403)".into()
        )));
    }
}
