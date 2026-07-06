use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("OIDC token rejected: {0}")]
    OidcRejected(String),
    #[error("repository not allowlisted: {0}")]
    RepoNotAllowlisted(String),
    #[error("environment not bound: {0}")]
    EnvironmentNotBound(String),
    #[error("ambiguous binding: {0}")]
    AmbiguousBinding(String),
    #[error("unsupported CI platform: {0}")]
    UnsupportedPlatform(String),
    #[error("rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("backend unavailable (HTTP {0})")]
    BackendUnavailable(String),
    #[error("plan not found: {0}")]
    PlanNotFound(String),
    #[error(
        "human approval required: {0}\n\
         Plan approval needs a verified human identity. Either approve the plan \
         in the Deslicer portal, or gate the CI job with a GitHub Environment \
         that requires reviewers so the CI proxy can attest the approver."
    )]
    HumanApprovalRequired(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("{0}")]
    Other(String),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::OidcRejected(_) => 4,
            CliError::RepoNotAllowlisted(_) => 5,
            CliError::EnvironmentNotBound(_) => 6,
            CliError::AmbiguousBinding(_) => 7,
            CliError::UnsupportedPlatform(_) => 8,
            CliError::RateLimited { .. } => 9,
            CliError::BackendUnavailable(_) | CliError::Transport(_) => 10,
            CliError::PlanNotFound(_) => 11,
            CliError::HumanApprovalRequired(_) => 12,
            CliError::Other(_) => 1,
        }
    }
}

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
