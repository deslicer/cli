use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("OIDC token rejected: {0}")]
    OidcRejected(String),
    #[error(
        "API token rejected: {0}\n\
         Check DESLICER_API_TOKEN (a tools-scope Observer API key from the portal), \
         or run `deslicer auth login` for interactive use."
    )]
    ApiTokenRejected(String),
    #[error("repository not allowlisted: {0}")]
    RepoNotAllowlisted(String),
    #[error("environment not bound: {0}")]
    EnvironmentNotBound(String),
    #[error("ambiguous binding: {0}")]
    AmbiguousBinding(String),
    #[error("git-backed change management: {0}")]
    GitBackedChangeManagement(String),
    #[error("unsupported CI platform: {0}")]
    UnsupportedPlatform(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("backend unavailable (HTTP {0})")]
    BackendUnavailable(String),
    #[error("plan not found: {0}")]
    PlanNotFound(String),
    #[error(
        "endpoint not available on this backend: {0}\n\
         This Observer host does not expose that API route. \
         Upgrade Observer or point DESLICER_API_URL / OBSERVER_API_URL at a host that supports the command."
    )]
    EndpointNotAvailable(String),
    #[error(
        "human approval required: {0}\n\
         Plan approval needs a verified human identity. Either approve the plan \
         in the Deslicer portal, or gate the CI job with a GitHub Environment \
         that requires reviewers so the CI proxy can attest the approver."
    )]
    HumanApprovalRequired(String),
    #[error("agent run failed: {0}")]
    AgentRunFailed(String),
    /// Command needs an interactive device session (`deslicer auth login`), not
    /// a static API token. Distinct from "not logged in" so agents get a stable
    /// `auth.device_session_required` code (issue #234).
    #[error("{0}")]
    DeviceSessionRequired(String),
    #[error("plan validation model unavailable")]
    ValidationModelUnavailable,
    #[error("plan validation timed out")]
    ValidationTimeout,
    #[error("plan validation unavailable")]
    ValidationUnavailable,
    #[error("transport error: {0}")]
    Transport(String),
    #[error("{0}")]
    Other(String),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::OidcRejected(_)
            | CliError::ApiTokenRejected(_)
            | CliError::DeviceSessionRequired(_) => 4,
            CliError::RepoNotAllowlisted(_) => 5,
            CliError::EnvironmentNotBound(_) => 6,
            CliError::AmbiguousBinding(_) | CliError::GitBackedChangeManagement(_) => 7,
            CliError::UnsupportedPlatform(_) => 8,
            CliError::InvalidInput(_) => 2,
            CliError::RateLimited { .. } => 9,
            CliError::BackendUnavailable(_) | CliError::Transport(_) => 10,
            CliError::PlanNotFound(_) => 11,
            CliError::HumanApprovalRequired(_) => 12,
            CliError::EndpointNotAvailable(_) => 15,
            CliError::AgentRunFailed(_) => 13,
            CliError::ValidationModelUnavailable => 22,
            CliError::ValidationTimeout => 23,
            CliError::ValidationUnavailable => 24,
            // Auth-class messages historically stuffed into Other (issue #220).
            CliError::Other(msg) if msg.contains("not logged in") => 4,
            CliError::Other(_) => 1,
        }
    }

    /// Whether agents should retry this failure class.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            CliError::RateLimited { .. }
                | CliError::BackendUnavailable(_)
                | CliError::Transport(_)
                | CliError::ValidationModelUnavailable
                | CliError::ValidationTimeout
                | CliError::ValidationUnavailable
        )
    }
}

/// Process exit codes published for agents (see `docs/exit-codes.md`).
/// Includes codes emitted outside `CliError` (14 lifecycle, 20/21 validation).
pub const PUBLISHED_EXIT_CODES: &[i32] = &[
    0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 20, 21, 22, 23, 24, 130,
];

impl From<crate::ci::OidcError> for CliError {
    fn from(err: crate::ci::OidcError) -> Self {
        match err {
            crate::ci::OidcError::MissingEnv(msg) => CliError::Other(msg),
            crate::ci::OidcError::Http(msg) => CliError::Transport(msg),
            crate::ci::OidcError::Unsupported(msg) => CliError::UnsupportedPlatform(msg),
            crate::ci::OidcError::Other(msg) => CliError::Other(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_errors() -> Vec<CliError> {
        vec![
            CliError::OidcRejected("x".into()),
            CliError::ApiTokenRejected("x".into()),
            CliError::RepoNotAllowlisted("x".into()),
            CliError::EnvironmentNotBound("x".into()),
            CliError::AmbiguousBinding("x".into()),
            CliError::GitBackedChangeManagement("x".into()),
            CliError::UnsupportedPlatform("x".into()),
            CliError::InvalidInput("x".into()),
            CliError::RateLimited {
                retry_after_secs: 1,
            },
            CliError::BackendUnavailable("503".into()),
            CliError::PlanNotFound("x".into()),
            CliError::EndpointNotAvailable("x".into()),
            CliError::HumanApprovalRequired("x".into()),
            CliError::AgentRunFailed("x".into()),
            CliError::DeviceSessionRequired("no active device session".into()),
            CliError::ValidationModelUnavailable,
            CliError::ValidationTimeout,
            CliError::ValidationUnavailable,
            CliError::Transport("x".into()),
            CliError::Other("not logged in".into()),
            CliError::Other("unexpected boom".into()),
        ]
    }

    #[test]
    fn not_logged_in_exits_4() {
        let err = CliError::Other(
            "not logged in. Run `deslicer auth login` and approve the code in the portal".into(),
        );
        assert_eq!(err.exit_code(), 4);
        assert!(!err.retryable());
    }

    #[test]
    fn every_cli_error_variant_maps_to_published_exit_code() {
        for err in sample_errors() {
            let code = err.exit_code();
            assert!(
                PUBLISHED_EXIT_CODES.contains(&code),
                "undocumented exit {code} for {err:?}"
            );
        }
    }

    #[test]
    fn exit_code_match_is_exhaustive_for_variants() {
        // Force a compile error when a new CliError variant is added without
        // updating exit_code() — mirror the production match arms.
        for err in sample_errors() {
            let _ = match &err {
                CliError::OidcRejected(_)
                | CliError::ApiTokenRejected(_)
                | CliError::RepoNotAllowlisted(_)
                | CliError::EnvironmentNotBound(_)
                | CliError::AmbiguousBinding(_)
                | CliError::GitBackedChangeManagement(_)
                | CliError::UnsupportedPlatform(_)
                | CliError::InvalidInput(_)
                | CliError::RateLimited { .. }
                | CliError::BackendUnavailable(_)
                | CliError::PlanNotFound(_)
                | CliError::EndpointNotAvailable(_)
                | CliError::HumanApprovalRequired(_)
                | CliError::AgentRunFailed(_)
                | CliError::DeviceSessionRequired(_)
                | CliError::ValidationModelUnavailable
                | CliError::ValidationTimeout
                | CliError::ValidationUnavailable
                | CliError::Transport(_)
                | CliError::Other(_) => err.exit_code(),
            };
        }
    }

    #[test]
    fn device_session_required_exits_4() {
        let err = CliError::DeviceSessionRequired(
            "no active device session. Run `deslicer auth login` first".into(),
        );
        assert_eq!(err.exit_code(), 4);
        assert!(!err.retryable());
    }
}
