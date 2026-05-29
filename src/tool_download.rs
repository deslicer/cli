use crate::errors::CliError;
use std::path::PathBuf;

pub async fn download_and_verify(
    base: &url::Url,
    token: &str,
    expected_sha256: &str,
) -> Result<PathBuf, CliError> {
    let _ = (base, token, expected_sha256);
    Err(CliError::Other("tool_download not implemented".to_string()))
}
