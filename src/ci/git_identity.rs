//! Repository URL + commit SHA from CI runner env (no OIDC required).
//!
//! Used when `change plan` talks to Observer with `DESLICER_API_TOKEN` instead
//! of the DAI CI proxy, which normally supplies these from the OIDC claims.

use super::CiPlatform;
use crate::errors::CliError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiGitIdentity {
    pub repository_url: String,
    pub commit_sha: String,
}

pub fn git_identity(platform: CiPlatform) -> Result<CiGitIdentity, CliError> {
    match platform {
        CiPlatform::Github => github_identity(),
        CiPlatform::Gitlab => gitlab_identity(),
        CiPlatform::Local => github_identity()
            .or_else(|_| gitlab_identity())
            .map_err(|_| {
                CliError::Other(
                    "git-sourced `change plan` with DESLICER_API_TOKEN needs \
                 GITHUB_REPOSITORY + GITHUB_SHA (or GitLab CI_PROJECT_URL + \
                 CI_COMMIT_SHA). In GitHub Actions these are set automatically. \
                 Otherwise use --source-dir + --target-group."
                        .into(),
                )
            }),
        CiPlatform::Azure | CiPlatform::Bitbucket => Err(CliError::Other(
            "git-sourced `change plan` with DESLICER_API_TOKEN is supported on \
             GitHub Actions and GitLab CI. Use OIDC (`deslicer change plan \
             --environment …`) or --source-dir + --target-group."
                .into(),
        )),
    }
}

fn github_identity() -> Result<CiGitIdentity, CliError> {
    let repo = required_trimmed("GITHUB_REPOSITORY")?;
    validate_repo_slug(&repo)?;
    let sha = required_trimmed("GITHUB_SHA")?;
    validate_commit_sha(&sha)?;
    let server = std::env::var("GITHUB_SERVER_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://github.com".to_string());
    validate_https_origin(&server, "GITHUB_SERVER_URL")?;
    Ok(CiGitIdentity {
        repository_url: format!("{server}/{repo}"),
        commit_sha: sha,
    })
}

fn gitlab_identity() -> Result<CiGitIdentity, CliError> {
    let repository_url = required_trimmed("CI_PROJECT_URL")?;
    validate_https_origin(&repository_url, "CI_PROJECT_URL")?;
    let sha = required_trimmed("CI_COMMIT_SHA")?;
    validate_commit_sha(&sha)?;
    Ok(CiGitIdentity {
        repository_url,
        commit_sha: sha,
    })
}

fn required_trimmed(name: &str) -> Result<String, CliError> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| CliError::Other(format!("missing environment variable: {name}")))
}

fn validate_repo_slug(slug: &str) -> Result<(), CliError> {
    let mut parts = slug.split('/');
    let owner = parts.next().unwrap_or("");
    let repo = parts.next().unwrap_or("");
    if owner.is_empty()
        || repo.is_empty()
        || parts.next().is_some()
        || slug.contains("..")
        || owner.starts_with('.')
        || repo.starts_with('.')
    {
        return Err(CliError::Other(
            "GITHUB_REPOSITORY must be owner/repo with no path traversal".into(),
        ));
    }
    Ok(())
}

fn validate_commit_sha(sha: &str) -> Result<(), CliError> {
    let valid_len = (7..=64).contains(&sha.len());
    if !valid_len || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CliError::Other(
            "commit SHA must be 7-64 hexadecimal characters".into(),
        ));
    }
    Ok(())
}

fn validate_https_origin(value: &str, env_name: &str) -> Result<(), CliError> {
    let url = url::Url::parse(value)
        .map_err(|_| CliError::Other(format!("{env_name} is not a valid URL")))?;
    if url.scheme() != "https" {
        return Err(CliError::Other(format!(
            "{env_name} must be an https:// URL"
        )));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(CliError::Other(format!(
            "{env_name} must not contain userinfo"
        )));
    }
    if url.host_str().is_none() {
        return Err(CliError::Other(format!("{env_name} is missing a host")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_git_env() {
        for name in [
            "GITHUB_REPOSITORY",
            "GITHUB_SHA",
            "GITHUB_SERVER_URL",
            "CI_PROJECT_URL",
            "CI_COMMIT_SHA",
        ] {
            std::env::remove_var(name);
        }
    }

    #[test]
    fn github_builds_github_com_url() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_git_env();
        std::env::set_var("GITHUB_REPOSITORY", "acme/splunk-config");
        std::env::set_var("GITHUB_SHA", "0123456789abcdef0123456789abcdef01234567");
        let id = git_identity(CiPlatform::Github).expect("identity");
        assert_eq!(id.repository_url, "https://github.com/acme/splunk-config");
        assert_eq!(id.commit_sha, "0123456789abcdef0123456789abcdef01234567");
        clear_git_env();
    }

    #[test]
    fn github_honours_server_url() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_git_env();
        std::env::set_var("GITHUB_REPOSITORY", "acme/splunk-config");
        std::env::set_var("GITHUB_SHA", "0123456");
        std::env::set_var("GITHUB_SERVER_URL", "https://git.example.com/");
        let id = git_identity(CiPlatform::Github).expect("identity");
        assert_eq!(
            id.repository_url,
            "https://git.example.com/acme/splunk-config"
        );
        clear_git_env();
    }

    #[test]
    fn gitlab_uses_project_url() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_git_env();
        std::env::set_var("CI_PROJECT_URL", "https://gitlab.com/acme/splunk-config");
        std::env::set_var("CI_COMMIT_SHA", "abcdef0");
        let id = git_identity(CiPlatform::Gitlab).expect("identity");
        assert_eq!(id.repository_url, "https://gitlab.com/acme/splunk-config");
        clear_git_env();
    }

    #[test]
    fn rejects_http_and_userinfo() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_git_env();
        std::env::set_var("CI_PROJECT_URL", "http://gitlab.com/acme/repo");
        std::env::set_var("CI_COMMIT_SHA", "abcdef0");
        assert!(git_identity(CiPlatform::Gitlab).is_err());
        std::env::set_var("CI_PROJECT_URL", "https://user:pass@gitlab.com/acme/repo");
        assert!(git_identity(CiPlatform::Gitlab).is_err());
        clear_git_env();
    }

    #[test]
    fn rejects_traversal_slug() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_git_env();
        std::env::set_var("GITHUB_REPOSITORY", "acme/../other");
        std::env::set_var("GITHUB_SHA", "abcdef0");
        assert!(git_identity(CiPlatform::Github).is_err());
        clear_git_env();
    }
}
