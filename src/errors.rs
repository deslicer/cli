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
            CliError::Other(_) => 1,
        }
    }
}
