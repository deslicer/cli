use crate::errors::CliError;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Download the tenant-config-processor binary, verify its SHA-256, and cache it locally.
pub async fn download_and_verify(
    base: &url::Url,
    token: &str,
    expected_sha256: &str,
) -> Result<PathBuf, CliError> {
    let cache_dir = resolve_cache_dir()?;
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| CliError::Transport(format!("create cache dir: {e}")))?;

    let cache_path = cache_file_path(&cache_dir, expected_sha256);

    if tokio::fs::try_exists(&cache_path)
        .await
        .map_err(|e| CliError::Transport(format!("stat cache file: {e}")))?
        && file_sha256_matches(&cache_path, expected_sha256).await?
    {
        return Ok(cache_path);
    }

    let bytes = fetch_tool_bytes(base, token).await?;
    let actual = sha256_hex(&bytes);
    if !sha256_eq(expected_sha256, &actual) {
        return Err(CliError::Other(format!(
            "checksum mismatch: expected {expected_sha256} got {actual}"
        )));
    }

    write_cache_atomic(&cache_path, &bytes).await?;
    Ok(cache_path)
}

fn resolve_cache_dir() -> Result<PathBuf, CliError> {
    if let Ok(dir) = std::env::var("DESLICER_CACHE_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let base = std::env::var("XDG_CACHE_HOME")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".cache"))
        })
        .ok_or_else(|| CliError::Other("cannot determine cache directory: HOME not set".into()))?;

    Ok(base.join("deslicer"))
}

fn cache_file_path(cache_dir: &Path, expected_sha256: &str) -> PathBuf {
    cache_dir.join(format!("tenant-config-processor-{expected_sha256}"))
}

async fn file_sha256_matches(path: &Path, expected_sha256: &str) -> Result<bool, CliError> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| CliError::Transport(format!("read cache file: {e}")))?;
    Ok(sha256_eq(expected_sha256, &sha256_hex(&bytes)))
}

async fn fetch_tool_bytes(base: &url::Url, token: &str) -> Result<Vec<u8>, CliError> {
    let url = base
        .join("api/v1/tools/download")
        .map_err(|e| CliError::Transport(format!("invalid download URL: {e}")))?;
    crate::http::assert_url_allowed(&url)?;

    let response = crate::http::client()
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| CliError::Transport(e.to_string()))?;

    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| CliError::Transport(e.to_string()))?
        .to_vec();

    if status.is_success() {
        return Ok(bytes);
    }

    if status.is_server_error() {
        return Err(CliError::BackendUnavailable(status.to_string()));
    }

    Err(CliError::Other(format!("HTTP {status}")))
}

async fn write_cache_atomic(final_path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let tmp_path = PathBuf::from(format!("{}.tmp", final_path.display()));
    tokio::fs::write(&tmp_path, bytes)
        .await
        .map_err(|e| CliError::Transport(format!("write temp cache file: {e}")))?;
    tokio::fs::rename(&tmp_path, final_path)
        .await
        .map_err(|e| CliError::Transport(format!("rename cache file: {e}")))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(final_path)
            .await
            .map_err(|e| CliError::Transport(format!("stat cache file: {e}")))?
            .permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(final_path, perms)
            .await
            .map_err(|e| CliError::Transport(format!("chmod cache file: {e}")))?;
    }

    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_eq(expected: &str, actual: &str) -> bool {
    expected.eq_ignore_ascii_case(actual)
}
