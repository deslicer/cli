use std::path::Path;
use std::process::Command;

use crate::errors::CliError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitProvider {
    Github,
    /// Path A2: Observer API token, no GitHub App OIDC.
    GithubToken,
    Gitlab,
    Azure,
    Bitbucket,
}

impl InitProvider {
    pub fn parse(raw: &str) -> Result<Self, CliError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "github" => Ok(Self::Github),
            "github-token" | "github_token" => Ok(Self::GithubToken),
            "gitlab" => Ok(Self::Gitlab),
            "azure" => Ok(Self::Azure),
            "bitbucket" => Ok(Self::Bitbucket),
            "auto" => Err(CliError::Other(
                "provider auto must be resolved from git remote origin".into(),
            )),
            other => Err(CliError::Other(format!(
                "unknown provider {other:?}; use github, github-token, gitlab, bitbucket, or azure"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::GithubToken => "github-token",
            Self::Gitlab => "gitlab",
            Self::Azure => "azure",
            Self::Bitbucket => "bitbucket",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginRepo {
    pub host: String,
    pub full_name: String,
    pub owner: String,
}

pub fn detect_provider(dir: &Path) -> Result<(InitProvider, OriginRepo), CliError> {
    let url = git_origin_url(dir)?;
    let origin = parse_origin_url(&url)?;
    let provider = provider_from_host(&origin.host)?;
    Ok((provider, origin))
}

pub fn origin_for_dir(dir: &Path) -> Result<OriginRepo, CliError> {
    let url = git_origin_url(dir)?;
    parse_origin_url(&url)
}

fn git_origin_url(dir: &Path) -> Result<String, CliError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|err| CliError::Other(format!("git remote get-url origin failed: {err}")))?;
    if !output.status.success() {
        return Err(CliError::Other(
            "no git remote named origin; pass --provider github|github-token|gitlab|bitbucket|azure"
                .into(),
        ));
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() {
        return Err(CliError::Other("git remote origin URL is empty".into()));
    }
    Ok(url)
}

pub fn parse_origin_url(url: &str) -> Result<OriginRepo, CliError> {
    let trimmed = url.trim();
    if let Some(rest) = trimmed.strip_prefix("git@") {
        let (host, path) = rest.split_once(':').ok_or_else(|| {
            CliError::Other("could not parse git@ origin URL (expected git@host:path)".into())
        })?;
        return origin_from_host_path(host, path);
    }
    let without_scheme = trimmed
        .strip_prefix("ssh://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let without_user = without_scheme
        .split_once('@')
        .map(|(_, rest)| rest)
        .unwrap_or(without_scheme);
    let (host, path) = without_user.split_once('/').ok_or_else(|| {
        CliError::Other("could not parse origin URL (expected host/owner/name)".into())
    })?;
    origin_from_host_path(host, path)
}

fn origin_from_host_path(host: &str, path: &str) -> Result<OriginRepo, CliError> {
    let host = host
        .split(':')
        .next()
        .unwrap_or(host)
        .trim()
        .to_ascii_lowercase();
    let cleaned = path.trim().trim_end_matches('/').trim_end_matches(".git");
    let segments: Vec<&str> = cleaned.split('/').filter(|part| !part.is_empty()).collect();
    if segments.len() < 2 {
        return Err(CliError::Other(
            "origin path must be owner/name (or group/project)".into(),
        ));
    }
    let owner = segments[0].to_string();
    let full_name = segments.join("/");
    Ok(OriginRepo {
        host,
        full_name,
        owner,
    })
}

fn provider_from_host(host: &str) -> Result<InitProvider, CliError> {
    if host == "github.com" || host.ends_with(".github.com") {
        return Ok(InitProvider::Github);
    }
    if host == "gitlab.com" || host.ends_with(".gitlab.com") {
        return Ok(InitProvider::Gitlab);
    }
    if host == "dev.azure.com" || host.ends_with(".visualstudio.com") {
        return Ok(InitProvider::Azure);
    }
    if host == "bitbucket.org" || host.ends_with(".bitbucket.org") {
        return Ok(InitProvider::Bitbucket);
    }
    Err(CliError::Other(format!(
        "unknown git host {host:?}; pass --provider github, github-token, gitlab, bitbucket, or azure"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_and_ssh_github() {
        let https = parse_origin_url("https://github.com/acme/splunk-config.git").unwrap();
        assert_eq!(https.owner, "acme");
        assert_eq!(https.full_name, "acme/splunk-config");
        assert_eq!(https.host, "github.com");
        let ssh = parse_origin_url("git@github.com:acme/splunk-config.git").unwrap();
        assert_eq!(ssh.full_name, "acme/splunk-config");
    }

    #[test]
    fn parses_nested_gitlab_path() {
        let origin = parse_origin_url("https://gitlab.com/group/sub/project.git").unwrap();
        assert_eq!(origin.full_name, "group/sub/project");
        assert_eq!(
            provider_from_host(&origin.host).unwrap(),
            InitProvider::Gitlab
        );
    }

    #[test]
    fn unknown_host_names_providers() {
        let err = provider_from_host("git.example.internal").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("github"));
        assert!(text.contains("github-token"));
        assert!(text.contains("gitlab"));
        assert!(text.contains("bitbucket"));
        assert!(text.contains("azure"));
    }

    #[test]
    fn parses_github_token_provider() {
        assert_eq!(
            InitProvider::parse("github-token").unwrap(),
            InitProvider::GithubToken
        );
        assert_eq!(InitProvider::GithubToken.as_str(), "github-token");
        // Auto-detect still maps github.com → OIDC Github, not token path.
        assert_eq!(
            provider_from_host("github.com").unwrap(),
            InitProvider::Github
        );
    }
}
