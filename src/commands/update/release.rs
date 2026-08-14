//! GitHub Releases metadata for self-update: tag resolution and artifact
//! naming for the compiled target triple.

use serde::Deserialize;

use crate::errors::CliError;

pub const REPO: &str = "deslicer/cli";

/// Target triple baked in at compile time by `build.rs`.
const TARGET: &str = env!("DESLICER_TARGET");

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
}

/// Resolve the latest stable tag. GitHub's `releases/latest` endpoint
/// excludes prereleases by definition, so rc/beta tags are never offered.
pub async fn resolve_latest_tag() -> Result<String, CliError> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = http_client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| CliError::Transport(format!("query latest release: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(CliError::Transport(format!(
            "GitHub API returned HTTP {status} for latest release"
        )));
    }

    let release: LatestRelease = response
        .json()
        .await
        .map_err(|e| CliError::Transport(format!("parse latest release: {e}")))?;
    validate_tag(&release.tag_name)
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

/// Release archive filename for the running platform, matching release.yml.
pub fn artifact_name() -> Result<String, CliError> {
    if TARGET.contains("windows") {
        // Windows archives are .zip; in-place replacement of a running .exe
        // is also blocked by the OS. Point users at the documented installs.
        return Err(CliError::Other(
            "self-update is not supported on Windows; download the new .zip \
             from https://github.com/deslicer/cli/releases"
                .into(),
        ));
    }
    Ok(format!("deslicer-{TARGET}.tar.gz"))
}

pub fn download_url(tag: &str, artifact: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/{tag}/{artifact}")
}

pub fn http_client() -> Result<reqwest::Client, CliError> {
    crate::http::try_client()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
