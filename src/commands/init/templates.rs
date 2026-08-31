use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha2::{Digest, Sha256};

use crate::errors::CliError;
use crate::observer_client::{BootstrapTemplates, Client};

use super::provider::InitProvider;

const MAX_PATH_CHARS: usize = 200;
const MAX_DECODED_BYTES: usize = 100 * 1024;

#[derive(Debug, Clone)]
pub struct DecodedTemplate {
    pub path: String,
    pub contents: Vec<u8>,
    pub if_missing: bool,
}

pub async fn load_templates(
    client: Option<&Client>,
    provider: InitProvider,
    offline: bool,
) -> Result<Vec<DecodedTemplate>, CliError> {
    if offline {
        return load_offline_cache(provider);
    }
    let Some(client) = client else {
        return Err(CliError::Other(
            "not logged in. Run `deslicer auth login` or pass --offline after a successful fetch"
                .into(),
        ));
    };
    let bundle = client.fetch_bootstrap_templates(provider.as_str()).await?;
    let decoded = decode_and_validate(provider, &bundle)?;
    write_offline_cache(provider, &bundle)?;
    Ok(decoded)
}

pub fn decode_and_validate(
    provider: InitProvider,
    bundle: &BootstrapTemplates,
) -> Result<Vec<DecodedTemplate>, CliError> {
    if bundle.provider != provider.as_str() {
        return Err(CliError::Other("template provider mismatch".into()));
    }
    validate_cache_sha(&bundle.tree_sha256)?;
    let mut decoded = Vec::with_capacity(bundle.files.len());
    let mut total = 0usize;
    for file in &bundle.files {
        validate_template_path(provider, &file.path)?;
        let contents = STANDARD
            .decode(file.contents_base64.as_bytes())
            .map_err(|_| CliError::Other(format!("invalid base64 for template {}", file.path)))?;
        total = total.saturating_add(contents.len());
        if total > MAX_DECODED_BYTES {
            return Err(CliError::Other(
                "template set exceeds 100 KiB decoded cap".into(),
            ));
        }
        let actual = sha256_hex(&contents);
        if !actual.eq_ignore_ascii_case(&file.sha256) {
            return Err(CliError::Other(format!(
                "template hash mismatch for {}",
                file.path
            )));
        }
        decoded.push(DecodedTemplate {
            if_missing: is_readme_path(&file.path),
            path: file.path.clone(),
            contents,
        });
    }
    Ok(decoded)
}

pub fn validate_template_path(provider: InitProvider, path: &str) -> Result<(), CliError> {
    if path.is_empty() {
        return Err(CliError::Other("template path is empty".into()));
    }
    if path.chars().count() > MAX_PATH_CHARS {
        return Err(CliError::Other(
            "template path exceeds 200 characters".into(),
        ));
    }
    if path.contains('\0') || path.starts_with('/') || path.starts_with('\\') || path.contains('~')
    {
        return Err(CliError::Other(
            "template path must be relative with no '~' or NUL".into(),
        ));
    }
    for segment in path.split(['/', '\\']) {
        if segment.is_empty() || segment == ".." {
            return Err(CliError::Other(
                "template path must not contain '..' or empty segments".into(),
            ));
        }
    }
    if !provider_allows_path(provider, path) {
        return Err(CliError::Other(format!(
            "template path {path} is not allowlisted for {}",
            provider.as_str()
        )));
    }
    Ok(())
}

fn provider_allows_path(provider: InitProvider, path: &str) -> bool {
    match provider {
        InitProvider::Github => {
            path == "README.md"
                || path.starts_with(".github/workflows/")
                || path.starts_with(".deslicer/")
        }
        InitProvider::Gitlab => path == ".gitlab-ci.yml" || path.starts_with(".deslicer/gitlab/"),
        InitProvider::Azure => path == "azure-pipelines.yml",
        InitProvider::Bitbucket => path == "bitbucket-pipelines.yml",
    }
}

fn is_readme_path(path: &str) -> bool {
    path == "README.md" || path == ".deslicer/environments/README.md"
}

fn validate_cache_sha(sha: &str) -> Result<(), CliError> {
    if sha.len() == 64 && sha.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Ok(());
    }
    Err(CliError::Other(
        "template tree sha must be 64 hexadecimal characters".into(),
    ))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn cache_root() -> Result<PathBuf, CliError> {
    if let Ok(dir) = std::env::var("DESLICER_CACHE_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed).join("bootstrap-templates"));
        }
    }
    let home = std::env::var("HOME").map_err(|_| CliError::Other("HOME is not set".into()))?;
    Ok(PathBuf::from(home)
        .join(".cache")
        .join("deslicer")
        .join("bootstrap-templates"))
}

fn cache_dir_for(provider: InitProvider, sha: &str) -> Result<PathBuf, CliError> {
    validate_cache_sha(sha)?;
    Ok(cache_root()?.join(provider.as_str()).join(sha))
}

fn write_offline_cache(
    provider: InitProvider,
    bundle: &BootstrapTemplates,
) -> Result<(), CliError> {
    let dir = cache_dir_for(provider, &bundle.tree_sha256)?;
    std::fs::create_dir_all(&dir)
        .map_err(|err| CliError::Other(format!("create template cache: {err}")))?;
    let json = serde_json::to_vec(bundle)
        .map_err(|err| CliError::Other(format!("serialize template cache: {err}")))?;
    std::fs::write(dir.join("bundle.json"), json)
        .map_err(|err| CliError::Other(format!("write template cache: {err}")))?;
    std::fs::write(
        cache_root()?.join(provider.as_str()).join("latest"),
        bundle.tree_sha256.as_bytes(),
    )
    .map_err(|err| CliError::Other(format!("write template cache pointer: {err}")))?;
    Ok(())
}

fn load_offline_cache(provider: InitProvider) -> Result<Vec<DecodedTemplate>, CliError> {
    let latest_path = cache_root()?.join(provider.as_str()).join("latest");
    let sha = std::fs::read_to_string(&latest_path).map_err(|_| {
        CliError::Other(format!(
            "no offline template cache for {}; run without --offline first",
            provider.as_str()
        ))
    })?;
    let sha = sha.trim().to_string();
    validate_cache_sha(&sha)?;
    let bundle_path = cache_dir_for(provider, &sha)?.join("bundle.json");
    let bytes = std::fs::read(&bundle_path).map_err(|_| {
        CliError::Other(format!(
            "offline template cache missing for {} / {sha}",
            provider.as_str()
        ))
    })?;
    let bundle: BootstrapTemplates = serde_json::from_slice(&bytes)
        .map_err(|err| CliError::Other(format!("invalid cached templates: {err}")))?;
    decode_and_validate(provider, &bundle)
}

/// Refuse writes that escape `root` after canonicalize (REQ-SEC-006).
pub fn resolve_write_path(root: &Path, relative: &str) -> Result<PathBuf, CliError> {
    let candidate = root.join(relative);
    if let Some(parent) = candidate.parent() {
        if parent.exists() {
            let canonical_parent = parent
                .canonicalize()
                .map_err(|err| CliError::Other(format!("canonicalize template parent: {err}")))?;
            if !canonical_parent.starts_with(root) {
                return Err(CliError::Other(
                    "refusing to write a template outside --dir".into(),
                ));
            }
        }
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observer_client::BootstrapTemplateFile;

    fn file(path: &str, contents: &str) -> BootstrapTemplateFile {
        let bytes = contents.as_bytes();
        BootstrapTemplateFile {
            path: path.to_string(),
            sha256: sha256_hex(bytes),
            contents_base64: STANDARD.encode(bytes),
        }
    }

    #[test]
    fn rejects_dotdot_and_absolute_paths() {
        assert!(validate_template_path(InitProvider::Github, "../x.yml").is_err());
        assert!(validate_template_path(InitProvider::Github, "/etc/passwd").is_err());
        assert!(validate_template_path(InitProvider::Gitlab, ".deslicer/gitlab/../../x").is_err());
    }

    #[test]
    fn rejects_non_hex_cache_sha() {
        assert!(validate_cache_sha("../abcdef").is_err());
        assert!(validate_cache_sha("abc").is_err());
        assert!(validate_cache_sha(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn decodes_valid_github_workflow() {
        let contents = "name: test\n";
        let bundle = BootstrapTemplates {
            provider: "github".into(),
            tree_sha256: "a".repeat(64),
            files: vec![file(".github/workflows/deslicer-plan.yml", contents)],
        };
        let decoded = decode_and_validate(InitProvider::Github, &bundle).unwrap();
        assert_eq!(decoded[0].contents, contents.as_bytes());
        assert!(!decoded[0].if_missing);
    }
}
