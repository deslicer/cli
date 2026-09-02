//! GitHub Releases tag resolution for self-update and installers.
//!
//! Tries the authenticated GitHub API when `GITHUB_TOKEN` / `GH_TOKEN` is set,
//! then falls back to the HTML `Location` redirect on `/releases/latest`.

use serde::Deserialize;
use url::Url;

use crate::errors::CliError;

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
}

/// Bearer token for GitHub API calls, if the environment provides one.
pub fn github_token() -> Option<String> {
    for name in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(value) = std::env::var(name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Resolve the latest stable release tag (excludes prereleases via GitHub semantics).
pub async fn resolve_latest_tag(client: &reqwest::Client, repo: &str) -> Result<String, CliError> {
    resolve_latest_tag_with_urls(
        client,
        &format!("https://api.github.com/repos/{repo}/releases/latest"),
        &format!("https://github.com/{repo}/releases/latest"),
    )
    .await
}

pub(crate) async fn resolve_latest_tag_with_urls(
    client: &reqwest::Client,
    api_url: &str,
    html_url: &str,
) -> Result<String, CliError> {
    if let Ok(tag) = resolve_latest_tag_via_api(client, api_url).await {
        return validate_tag(&tag);
    }
    if let Ok(tag) = resolve_latest_tag_via_html(client, html_url).await {
        return validate_tag(&tag);
    }
    Err(CliError::Transport(
        "could not resolve latest release tag (GitHub API rate-limited or unavailable); \
         set GITHUB_TOKEN or GH_TOKEN, or pass `--version vX.Y.Z`"
            .into(),
    ))
}

async fn resolve_latest_tag_via_api(
    client: &reqwest::Client,
    api_url: &str,
) -> Result<String, CliError> {
    let mut request = client.get(api_url);
    if let Some(token) = github_token() {
        request = request
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json");
    }
    let response = request
        .send()
        .await
        .map_err(|e| CliError::Transport(format!("query latest release: {e}")))?;

    if !response.status().is_success() {
        return Err(CliError::Transport(format!(
            "GitHub API returned HTTP {} for latest release",
            response.status()
        )));
    }

    let release: LatestRelease = response
        .json()
        .await
        .map_err(|e| CliError::Transport(format!("parse latest release: {e}")))?;
    Ok(release.tag_name)
}

async fn resolve_latest_tag_via_html(
    client: &reqwest::Client,
    html_url: &str,
) -> Result<String, CliError> {
    let response = client
        .head(html_url)
        .send()
        .await
        .map_err(|e| CliError::Transport(format!("query latest release redirect: {e}")))?;

    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            CliError::Transport(format!(
                "GitHub HTML latest redirect missing Location header (HTTP {})",
                response.status()
            ))
        })?;

    tag_from_release_location(location)
}

fn tag_from_release_location(location: &str) -> Result<String, CliError> {
    let parsed = Url::parse(location)
        .map_err(|e| CliError::Transport(format!("parse release redirect: {e}")))?;
    let tag = parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back().map(str::to_string))
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| {
            CliError::Transport(format!("release redirect has no tag segment: {location}"))
        })?;
    Ok(tag)
}

/// Accept only plain semver tags (vX.Y.Z with optional prerelease suffix of
/// alphanumerics, dots, and hyphens) so a tag can never smuggle path or URL
/// metacharacters into the download URL (REQ-SEC-006).
pub fn validate_tag(tag: &str) -> Result<String, CliError> {
    let trimmed = tag.trim();
    let body = trimmed.strip_prefix('v').unwrap_or("");
    let valid = !body.is_empty()
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        && body.chars().next().is_some_and(|c| c.is_ascii_digit());
    if valid {
        Ok(trimmed.to_string())
    } else {
        Err(CliError::Other(format!(
            "invalid release tag {trimmed:?}: expected vX.Y.Z"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client")
    }

    #[test]
    fn accepts_plain_semver_tags() {
        assert_eq!(validate_tag("v1.2.3").ok().as_deref(), Some("v1.2.3"));
        assert_eq!(
            validate_tag(" v1.0.0-rc.1 ").ok().as_deref(),
            Some("v1.0.0-rc.1")
        );
    }

    #[test]
    fn rejects_malformed_tags() {
        for bad in ["", "1.2.3", "v", "v../..", "v1.0.0/evil", "v1.0.0?x=1"] {
            assert!(validate_tag(bad).is_err(), "tag {bad:?} should be rejected");
        }
    }

    #[test]
    fn tag_from_release_location_extracts_tag() {
        let tag = tag_from_release_location("https://github.com/acme/widget/releases/tag/v1.3.1")
            .expect("tag");
        assert_eq!(tag, "v1.3.1");
    }

    #[tokio::test]
    async fn resolve_latest_tag_uses_api_when_available() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/widget/releases/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tag_name": "v2.0.0"
            })))
            .mount(&server)
            .await;

        let api_url = format!("{}/repos/acme/widget/releases/latest", server.uri());
        let tag = resolve_latest_tag_with_urls(&test_client(), &api_url, &api_url)
            .await
            .expect("tag");
        assert_eq!(tag, "v2.0.0");
    }

    #[tokio::test]
    async fn resolve_latest_tag_falls_back_to_html_on_api_403() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/widget/releases/latest"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("HEAD"))
            .and(path("/acme/widget/releases/latest"))
            .respond_with(ResponseTemplate::new(302).insert_header(
                "Location",
                "https://github.com/acme/widget/releases/tag/v8.8.8",
            ))
            .mount(&server)
            .await;

        let api_url = format!("{}/repos/acme/widget/releases/latest", server.uri());
        let html_url = format!("{}/acme/widget/releases/latest", server.uri());
        let tag = resolve_latest_tag_with_urls(&test_client(), &api_url, &html_url)
            .await
            .expect("tag");
        assert_eq!(tag, "v8.8.8");
    }

    #[tokio::test]
    async fn resolve_latest_tag_sends_bearer_token_when_set() {
        {
            let _guard = ENV_LOCK.lock().expect("env lock");
            std::env::set_var("GITHUB_TOKEN", "gh-test-token");
        }

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/widget/releases/latest"))
            .and(header("Authorization", "Bearer gh-test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tag_name": "v3.0.0"
            })))
            .mount(&server)
            .await;

        let api_url = format!("{}/repos/acme/widget/releases/latest", server.uri());
        let tag = resolve_latest_tag_with_urls(&test_client(), &api_url, &api_url)
            .await
            .expect("tag");
        assert_eq!(tag, "v3.0.0");

        {
            let _guard = ENV_LOCK.lock().expect("env lock");
            std::env::remove_var("GITHUB_TOKEN");
        }
    }

    #[tokio::test]
    async fn resolve_latest_tag_clear_error_when_both_fail() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/widget/releases/latest"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("HEAD"))
            .and(path("/acme/widget/releases/latest"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let api_url = format!("{}/repos/acme/widget/releases/latest", server.uri());
        let html_url = format!("{}/acme/widget/releases/latest", server.uri());
        let err = resolve_latest_tag_with_urls(&test_client(), &api_url, &html_url)
            .await
            .expect_err("expected failure");
        assert!(err.to_string().contains("GITHUB_TOKEN"));
    }

    #[test]
    fn github_token_prefers_github_token_env() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::set_var("GITHUB_TOKEN", "gh-test");
        std::env::set_var("GH_TOKEN", "other");
        assert_eq!(github_token().as_deref(), Some("gh-test"));
        std::env::remove_var("GITHUB_TOKEN");
        std::env::remove_var("GH_TOKEN");
    }
}
