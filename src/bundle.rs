//! Source bundle packaging for the GitHub-App-free plan flow.
//!
//! Packages a local directory of Splunk app configs into a deterministic
//! tar.gz and computes its SHA-256 (REQ-SIGN-008). The digest is declared on
//! upload, re-verified by Observer, and re-verified again by the
//! compile-runner before extraction — the plan is pinned to exactly these
//! bytes end to end.

use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};

use crate::errors::CliError;

/// Mirror of Observer's route-level cap (`plan_sources::MAX_BUNDLE_BYTES`).
pub const MAX_BUNDLE_BYTES: usize = 32 * 1024 * 1024;

/// Directory names that never belong in a source bundle.
const SKIPPED_DIRS: &[&str] = &[".git", ".svn", ".hg", "node_modules", "__pycache__"];

pub struct PackagedBundle {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub file_count: usize,
}

// Manual Debug: never dump the raw archive bytes (config files may contain
// credentials the operator would not expect to see in logs — REQ-LOG-007).
impl std::fmt::Debug for PackagedBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackagedBundle")
            .field("bytes_len", &self.bytes.len())
            .field("sha256", &self.sha256)
            .field("file_count", &self.file_count)
            .finish()
    }
}

/// Package `dir` into a gzip'd tar with paths relative to `dir`.
///
/// Symlinks are skipped (a symlink escaping the tree would otherwise leak
/// unrelated files into the upload), as are VCS/dependency directories.
pub fn package_directory(dir: &Path) -> Result<PackagedBundle, CliError> {
    let root = dir.canonicalize().map_err(|e| {
        CliError::Other(format!("cannot resolve source dir {}: {e}", dir.display()))
    })?;
    if !root.is_dir() {
        return Err(CliError::Other(format!(
            "source path is not a directory: {}",
            root.display()
        )));
    }

    let mut files = Vec::new();
    collect_files(&root, &root, &mut files)?;
    if files.is_empty() {
        return Err(CliError::Other(format!(
            "source directory contains no files: {}",
            root.display()
        )));
    }
    // Deterministic entry order → reproducible digest for unchanged content.
    files.sort();

    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for relative in &files {
        let absolute = root.join(relative);
        builder
            .append_path_with_name(&absolute, relative)
            .map_err(|e| {
                CliError::Other(format!(
                    "failed to add {} to bundle: {e}",
                    relative.display()
                ))
            })?;
    }
    let bytes = builder
        .into_inner()
        .and_then(|encoder| encoder.finish())
        .map_err(|e| CliError::Other(format!("failed to finalise bundle archive: {e}")))?;

    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(CliError::Other(format!(
            "bundle is {} bytes compressed — exceeds the {} MiB upload limit",
            bytes.len(),
            MAX_BUNDLE_BYTES / (1024 * 1024)
        )));
    }

    let sha256 = hex::encode(Sha256::digest(&bytes));
    Ok(PackagedBundle {
        bytes,
        sha256,
        file_count: files.len(),
    })
}

fn collect_files(root: &Path, current: &Path, out: &mut Vec<PathBuf>) -> Result<(), CliError> {
    let entries = std::fs::read_dir(current)
        .map_err(|e| CliError::Other(format!("cannot read {}: {e}", current.display())))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| CliError::Other(format!("cannot read directory entry: {e}")))?;
        let path = entry.path();
        // symlink_metadata: do not follow links — a symlink out of the tree
        // must not pull external files into the upload.
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|e| CliError::Other(format!("cannot stat {}: {e}", path.display())))?;

        if metadata.is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if SKIPPED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_files(root, &path, out)?;
        } else if metadata.is_file() {
            let relative = path.strip_prefix(root).map_err(|_| {
                CliError::Other(format!("path {} escaped source root", path.display()))
            })?;
            out.push(relative.to_path_buf());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, relative: &str, contents: &str) {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn packages_files_and_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "apps/demo/default/app.conf", "[install]\n");
        write(dir.path(), "apps/demo/default/props.conf", "[src]\n");

        let first = package_directory(dir.path()).unwrap();
        let second = package_directory(dir.path()).unwrap();
        assert_eq!(first.file_count, 2);
        assert_eq!(first.sha256, second.sha256);
    }

    #[test]
    fn skips_git_directories_and_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "apps/demo/default/app.conf", "[install]\n");
        write(dir.path(), ".git/config", "[core]\n");
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", dir.path().join("link")).unwrap();

        let packaged = package_directory(dir.path()).unwrap();
        assert_eq!(packaged.file_count, 1);
    }

    #[test]
    fn rejects_empty_directories() {
        let dir = tempfile::tempdir().unwrap();
        let err = package_directory(dir.path()).unwrap_err();
        assert!(err.to_string().contains("no files"));
    }
}
