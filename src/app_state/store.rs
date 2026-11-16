//! Local stand-in for an immutable object store.
//!
//! Objects are stored by SHA-256 and never overwritten. A `file` link is the
//! local equivalent of a pre-signed URL: the caller fetches bytes by digest.
//! A real RustFS / DTS pre-signed link is platform work and is not spoken here.

use std::fs;
use std::path::{Path, PathBuf};

use super::fingerprint::{self, Fingerprint};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub link: String,
    pub already_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub fingerprint: String,
    pub objects: Vec<StoredObject>,
}

pub fn publish(app_dir: &Path, store: &Path) -> Result<Published, String> {
    let fingerprint = fingerprint::fingerprint_dir(app_dir)?;
    let mut objects = Vec::new();
    for file in &fingerprint.files {
        objects.push(put_object(app_dir, store, file)?);
    }
    write_manifest(store, &fingerprint)?;
    Ok(Published {
        fingerprint: fingerprint.fingerprint,
        objects,
    })
}

pub fn fetch(store: &Path, digest: &str) -> Result<Vec<u8>, String> {
    if !is_sha256(digest) {
        return Err(format!("digest {digest:?} is not a sha256 hex string"));
    }
    let path = fingerprint::object_path(store, digest);
    fs::read(&path).map_err(|_| format!("object {digest} is not in {}", store.display()))
}

fn put_object(
    app_dir: &Path,
    store: &Path,
    file: &fingerprint::FileDigest,
) -> Result<StoredObject, String> {
    let source = app_dir.join(&file.path);
    let bytes = fs::read(&source).map_err(|err| format!("read {}: {err}", source.display()))?;
    if fingerprint::hex_sha256(&bytes) != file.sha256 {
        return Err(format!("{} changed while it was being stored", file.path));
    }
    let dest = fingerprint::object_path(store, &file.sha256);
    let already_present = dest.is_file();
    if !already_present {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(&dest, &bytes).map_err(|err| format!("write {}: {err}", dest.display()))?;
    }
    Ok(StoredObject {
        path: file.path.clone(),
        sha256: file.sha256.clone(),
        bytes: file.bytes,
        link: link_for(&dest),
        already_present,
    })
}

fn write_manifest(store: &Path, fingerprint: &Fingerprint) -> Result<(), String> {
    fs::create_dir_all(store).map_err(|err| err.to_string())?;
    let body = serde_json::to_string_pretty(&manifest_value(fingerprint))
        .map_err(|err| err.to_string())?;
    fs::write(store.join("manifest.json"), body).map_err(|err| err.to_string())
}

fn manifest_value(fingerprint: &Fingerprint) -> serde_json::Value {
    serde_json::json!({
        "algorithm": "sha256",
        "fingerprint": fingerprint.fingerprint,
        "files": fingerprint.files.iter().map(|file| serde_json::json!({
            "path": file.path,
            "sha256": file.sha256,
            "bytes": file.bytes,
        })).collect::<Vec<_>>(),
    })
}

fn link_for(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn is_sha256(digest: &str) -> bool {
    digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn manifest_path(store: &Path) -> PathBuf {
    store.join("manifest.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_is_immutable_and_fetch_returns_the_changed_object() {
        let app = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        fs::write(app.path().join("app.conf"), "a=1\n").unwrap();
        let first = publish(app.path(), store.path()).unwrap();
        assert_eq!(first.objects.len(), 1);
        assert!(!first.objects[0].already_present);
        let again = publish(app.path(), store.path()).unwrap();
        assert!(again.objects[0].already_present);
        assert_eq!(again.objects[0].sha256, first.objects[0].sha256);

        fs::write(app.path().join("app.conf"), "a=2\n").unwrap();
        let drifted = publish(app.path(), store.path()).unwrap();
        assert_ne!(drifted.fingerprint, first.fingerprint);
        assert!(!drifted.objects[0].already_present);
        let bytes = fetch(store.path(), &drifted.objects[0].sha256).unwrap();
        assert_eq!(bytes, b"a=2\n");
        let original = fetch(store.path(), &first.objects[0].sha256).unwrap();
        assert_eq!(original, b"a=1\n");
    }
}
