//! Download, verify, and atomically install a release binary over the
//! currently running executable.

use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use super::release;
use crate::errors::CliError;

/// 100 MiB cap on both the downloaded archive and the extracted binary —
/// release binaries are ~10 MiB, so anything near the cap is malformed.
const MAX_ARTIFACT_BYTES: u64 = 100 * 1024 * 1024;

pub async fn download_and_replace(tag: &str) -> Result<(), CliError> {
    let artifact = release::artifact_name()?;
    let client = release::http_client()?;

    let archive_url = release::download_url(tag, &artifact);
    println!("downloading {archive_url}");
    let archive = fetch_capped(&client, &archive_url).await?;

    let sidecar_url = format!("{archive_url}.sha256");
    let sidecar = fetch_capped(&client, &sidecar_url).await?;
    verify_sha256(&archive, &sidecar)?;
    println!("checksum verified");

    let binary = extract_binary(&archive)?;
    replace_current_exe(&binary).await
}

async fn fetch_capped(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, CliError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| CliError::Transport(format!("download {url}: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(CliError::Transport(format!("HTTP {status} for {url}")));
    }
    if response.content_length().unwrap_or(0) > MAX_ARTIFACT_BYTES {
        return Err(CliError::Other(format!(
            "artifact at {url} exceeds the {MAX_ARTIFACT_BYTES}-byte limit"
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| CliError::Transport(format!("read body of {url}: {e}")))?;
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(CliError::Other(format!(
            "artifact at {url} exceeds the {MAX_ARTIFACT_BYTES}-byte limit"
        )));
    }
    Ok(bytes.to_vec())
}

fn verify_sha256(archive: &[u8], sidecar: &[u8]) -> Result<(), CliError> {
    let text = std::str::from_utf8(sidecar)
        .map_err(|_| CliError::Other("sha256 sidecar is not valid UTF-8".into()))?;
    let expected = text
        .split_whitespace()
        .next()
        .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| CliError::Other("sha256 sidecar has no valid digest".into()))?;

    let digest = Sha256::digest(archive);
    let actual = hex::encode(digest);
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(CliError::Other(format!(
            "checksum mismatch: expected {expected}, got {actual}"
        )))
    }
}

/// Pull the `deslicer` entry out of the tar.gz archive with a decompression
/// cap, mirroring the bundle extractor's fail-closed posture.
fn extract_binary(archive: &[u8]) -> Result<Vec<u8>, CliError> {
    let decoder = GzDecoder::new(archive);
    let mut tar = tar::Archive::new(decoder.take(MAX_ARTIFACT_BYTES));

    let entries = tar
        .entries()
        .map_err(|e| CliError::Other(format!("read archive: {e}")))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| CliError::Other(format!("read archive entry: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| CliError::Other(format!("read entry path: {e}")))?;
        if path.file_name().and_then(|n| n.to_str()) != Some("deslicer") {
            continue;
        }
        let mut binary = Vec::new();
        entry
            .read_to_end(&mut binary)
            .map_err(|e| CliError::Other(format!("extract binary: {e}")))?;
        if binary.is_empty() {
            return Err(CliError::Other("extracted binary is empty".into()));
        }
        return Ok(binary);
    }
    Err(CliError::Other(
        "archive did not contain the deslicer binary".into(),
    ))
}

/// Write the new binary next to the current executable and rename it over —
/// rename is atomic within a filesystem, and Unix permits replacing the file
/// backing a running process.
async fn replace_current_exe(binary: &[u8]) -> Result<(), CliError> {
    let current = std::env::current_exe()
        .map_err(|e| CliError::Other(format!("resolve current executable: {e}")))?;
    let staging = staging_path(&current);

    tokio::fs::write(&staging, binary)
        .await
        .map_err(|e| CliError::Other(format!("write {}: {e}", staging.display())))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))
            .await
            .map_err(|e| CliError::Other(format!("chmod {}: {e}", staging.display())))?;
    }

    if let Err(e) = tokio::fs::rename(&staging, &current).await {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(CliError::Other(format!(
            "install to {} failed: {e} (is the directory writable? re-run the \
             installer with sudo, or use your package manager)",
            current.display()
        )));
    }
    Ok(())
}

fn staging_path(current: &Path) -> PathBuf {
    let file_name = current
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "deslicer".to_string());
    current.with_file_name(format!(".{file_name}.update-{}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tar_gz_with(name: &str, contents: &[u8]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, name, contents)
            .expect("append tar entry");
        builder
            .into_inner()
            .expect("finish tar")
            .finish()
            .expect("finish gzip")
    }

    #[test]
    fn extracts_the_deslicer_entry() {
        let archive = tar_gz_with("deslicer", b"fake-binary");
        assert_eq!(extract_binary(&archive).expect("extract"), b"fake-binary");
    }

    #[test]
    fn rejects_archive_without_binary() {
        let archive = tar_gz_with("README.md", b"not a binary");
        assert!(extract_binary(&archive).is_err());
    }

    #[test]
    fn verify_sha256_accepts_matching_digest() {
        let data = b"payload";
        let digest = hex::encode(Sha256::digest(data));
        let sidecar = format!("{digest}  deslicer-x.tar.gz\n");
        assert!(verify_sha256(data, sidecar.as_bytes()).is_ok());
    }

    #[test]
    fn verify_sha256_rejects_mismatch_and_garbage() {
        let good = hex::encode(Sha256::digest(b"other"));
        assert!(verify_sha256(b"payload", format!("{good}  f").as_bytes()).is_err());
        assert!(verify_sha256(b"payload", b"not-a-digest").is_err());
    }
}
