//! GitHub Releases metadata for self-update: tag resolution and artifact
//! naming for the compiled target triple.

use crate::errors::CliError;
use crate::github_release;

pub use crate::github_release::validate_tag;

pub const REPO: &str = "deslicer/cli";

/// Target triple baked in at compile time by `build.rs`.
const TARGET: &str = env!("DESLICER_TARGET");

/// Resolve the latest stable tag. GitHub's `releases/latest` endpoint
/// excludes prereleases by definition, so rc/beta tags are never offered.
pub async fn resolve_latest_tag() -> Result<String, CliError> {
    github_release::resolve_latest_tag(&http_client()?, REPO).await
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
