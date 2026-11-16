//! Content fingerprint of a Splunk app directory.
//!
//! The fingerprint is the SHA-256 of the sorted `path digest` lines. A file
//! change changes that file's object digest and the fingerprint. Hidden files
//! and `*.pyc` are skipped so local editor noise does not look like drift.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDigest {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub files: Vec<FileDigest>,
    pub fingerprint: String,
}

pub fn fingerprint_dir(root: &Path) -> Result<Fingerprint, String> {
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let fingerprint = rollup(&files);
    Ok(Fingerprint { files, fingerprint })
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<FileDigest>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|err| format!("read {}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "__pycache__" {
            continue;
        }
        if path.is_dir() {
            collect(root, &path, out)?;
            continue;
        }
        if name.ends_with(".pyc") {
            continue;
        }
        out.push(digest_file(root, &path)?);
    }
    Ok(())
}

fn digest_file(root: &Path, path: &Path) -> Result<FileDigest, String> {
    let bytes = fs::read(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let relative = relative_path(root, path)?;
    Ok(FileDigest {
        path: relative,
        sha256: hex_sha256(&bytes),
        bytes: bytes.len() as u64,
    })
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| format!("{} is outside {}", path.display(), root.display()))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn rollup(files: &[FileDigest]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update(b" ");
        hasher.update(file.sha256.as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn object_path(store: &Path, digest: &str) -> PathBuf {
    store.join("objects").join(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_a_file_changes_only_that_digest_and_the_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("app.conf"), "a=1\n").unwrap();
        fs::create_dir(dir.path().join("default")).unwrap();
        fs::write(dir.path().join("default/indexes.conf"), "[main]\n").unwrap();
        fs::write(dir.path().join(".hidden"), "nope").unwrap();

        let first = fingerprint_dir(dir.path()).unwrap();
        assert_eq!(first.files.len(), 2);
        let again = fingerprint_dir(dir.path()).unwrap();
        assert_eq!(first, again);

        fs::write(dir.path().join("app.conf"), "a=2\n").unwrap();
        let changed = fingerprint_dir(dir.path()).unwrap();
        assert_ne!(changed.fingerprint, first.fingerprint);
        let old_index = first
            .files
            .iter()
            .find(|file| file.path == "default/indexes.conf")
            .unwrap();
        let new_index = changed
            .files
            .iter()
            .find(|file| file.path == "default/indexes.conf")
            .unwrap();
        assert_eq!(old_index.sha256, new_index.sha256);
    }
}
