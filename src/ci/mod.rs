pub mod azure;
pub mod bitbucket;
pub mod git_identity;
pub mod github;
pub mod gitlab;
pub mod local;

pub use git_identity::{git_identity, CiGitIdentity};

pub const AUDIENCE: &str = "https://api.deslicer.ai";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiPlatform {
    Github,
    Gitlab,
    Azure,
    Bitbucket,
    Local,
}

impl CiPlatform {
    pub fn header_value(self) -> &'static str {
        match self {
            CiPlatform::Github => "github",
            CiPlatform::Gitlab => "gitlab",
            CiPlatform::Azure => "azure",
            CiPlatform::Bitbucket => "bitbucket",
            CiPlatform::Local => "local",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    #[error("missing environment variable: {0}")]
    MissingEnv(String),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
}

#[async_trait::async_trait]
pub trait OidcTokenProvider: Send + Sync {
    async fn fetch_token(&self, audience: &str) -> Result<String, OidcError>;
}

pub fn detect_platform(override_platform: Option<CiPlatform>) -> CiPlatform {
    if let Some(platform) = override_platform {
        return platform;
    }
    if std::env::var("GITHUB_ACTIONS").is_ok() {
        return CiPlatform::Github;
    }
    if std::env::var("GITLAB_CI").is_ok() {
        return CiPlatform::Gitlab;
    }
    if std::env::var("TF_BUILD").is_ok() {
        return CiPlatform::Azure;
    }
    if std::env::var("BITBUCKET_BUILD_NUMBER").is_ok() {
        return CiPlatform::Bitbucket;
    }
    CiPlatform::Local
}

pub fn provider_for(platform: CiPlatform) -> Box<dyn OidcTokenProvider> {
    match platform {
        CiPlatform::Github => Box::new(github::GithubProvider),
        CiPlatform::Gitlab => Box::new(gitlab::GitlabProvider),
        CiPlatform::Azure => Box::new(azure::AzureProvider),
        CiPlatform::Bitbucket => Box::new(bitbucket::BitbucketProvider),
        CiPlatform::Local => Box::new(local::LocalProvider),
    }
}
