//! Resolve the tenant environment filename stem and locate env YAML files.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::environment_name::{derive_environment_name, is_valid_environment_name};
use crate::environment_yaml::DESLICER_ENVIRONMENTS_DIR;
use crate::errors::CliError;

/// Missing-stem message used by init, inventory sync, and token-path plan.
pub const MISSING_ENVIRONMENT_HINT: &str = "pass --environment <tenant-slug> \
(this becomes the GitHub Environment name and DESLICER_ENVIRONMENT)";

/// Resolve the environment filename stem.
///
/// 1. `--environment` (run through [`derive_environment_name`])
/// 2. device-session `tenant_slug` when present
/// 3. exactly one `.deslicer/environments/*.{yml,yaml}` under the search roots
/// 4. otherwise a clear error
pub fn resolve_environment_stem(
    explicit: Option<&str>,
    tenant_slug: Option<&str>,
    search_roots: &[&Path],
) -> Result<ResolvedStem, CliError> {
    if let Some(raw) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(ResolvedStem {
            stem: derive_environment_name(raw),
            label: raw.to_string(),
        });
    }
    if let Some(slug) = tenant_slug.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(ResolvedStem {
            stem: derive_environment_name(slug),
            label: slug.to_string(),
        });
    }
    let stems = list_environment_stems(search_roots)?;
    match stems.as_slice() {
        [only] => Ok(ResolvedStem {
            stem: only.clone(),
            label: only.clone(),
        }),
        [] => Err(CliError::Other(format!(
            "could not resolve a tenant environment file; {MISSING_ENVIRONMENT_HINT}"
        ))),
        _ => Err(CliError::Other(format!(
            "multiple environment files found ({}); {MISSING_ENVIRONMENT_HINT}",
            stems.join(", ")
        ))),
    }
}

/// Optional resolution for `change plan`: no files → `None`, many files → error.
pub fn resolve_optional_environment_stem(
    explicit: Option<&str>,
    tenant_slug: Option<&str>,
    search_roots: &[&Path],
) -> Result<Option<ResolvedStem>, CliError> {
    if explicit.is_some() || tenant_slug.is_some() {
        return resolve_environment_stem(explicit, tenant_slug, search_roots).map(Some);
    }
    let stems = list_environment_stems(search_roots)?;
    match stems.as_slice() {
        [only] => Ok(Some(ResolvedStem {
            stem: only.clone(),
            label: only.clone(),
        })),
        [] => Ok(None),
        _ => Err(CliError::Other(format!(
            "multiple environment files found ({}); {MISSING_ENVIRONMENT_HINT}",
            stems.join(", ")
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStem {
    pub stem: String,
    pub label: String,
}

/// Absolute path for `.deslicer/environments/<stem>.yml` under `root`.
pub fn environment_file_on_disk(root: &Path, stem: &str) -> PathBuf {
    root.join(DESLICER_ENVIRONMENTS_DIR)
        .join(format!("{stem}.yml"))
}

/// Filename stems of `*.yml` / `*.yaml` under each root's environments dir.
/// Deduped, first-seen order. Skips invalid Observer names and `README`.
pub fn list_environment_stems(search_roots: &[&Path]) -> Result<Vec<String>, CliError> {
    let mut seen = std::collections::HashSet::new();
    let mut stems = Vec::new();
    for root in search_roots {
        collect_stems_from_root(root, &mut seen, &mut stems)?;
    }
    Ok(stems)
}

fn collect_stems_from_root(
    root: &Path,
    seen: &mut std::collections::HashSet<String>,
    stems: &mut Vec<String>,
) -> Result<(), CliError> {
    let dir = root.join(DESLICER_ENVIRONMENTS_DIR);
    if !dir.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(&dir)
        .map_err(|err| CliError::Other(format!("read {}: {err}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|err| CliError::Other(format!("read env dir: {err}")))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(stem) = stem_from_env_path(&path) else {
            continue;
        };
        if seen.insert(stem.clone()) {
            stems.push(stem);
        }
    }
    Ok(())
}

fn stem_from_env_path(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    if !matches!(ext, "yml" | "yaml") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?.to_string();
    if stem.eq_ignore_ascii_case("README") || !is_valid_environment_name(&stem) {
        return None;
    }
    Some(stem)
}

/// `git rev-parse --show-toplevel` from `dir`, if this is a git checkout.
pub fn git_toplevel(dir: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

/// Unique search roots: `dir` then its git toplevel when different.
pub fn search_roots_for(dir: &Path) -> Vec<PathBuf> {
    let mut roots = vec![dir.to_path_buf()];
    if let Some(top) = git_toplevel(dir) {
        if top != dir {
            roots.push(top);
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn explicit_environment_wins() {
        let resolved = resolve_environment_stem(Some("Acme Prod"), None, &[]).unwrap();
        assert_eq!(resolved.stem, "Acme-Prod");
        assert_eq!(resolved.label, "Acme Prod");
    }

    #[test]
    fn tenant_slug_used_when_no_flag() {
        let resolved = resolve_environment_stem(None, Some("acme-prod"), &[]).unwrap();
        assert_eq!(resolved.stem, "acme-prod");
    }

    #[test]
    fn single_existing_file_is_used() {
        let dir = tempdir().unwrap();
        let env_dir = dir.path().join(DESLICER_ENVIRONMENTS_DIR);
        std::fs::create_dir_all(&env_dir).unwrap();
        std::fs::write(env_dir.join("acme-prod.yml"), "destinations: []\n").unwrap();
        let resolved = resolve_environment_stem(None, None, &[dir.path()]).unwrap();
        assert_eq!(resolved.stem, "acme-prod");
    }

    #[test]
    fn multiple_files_error_without_flag() {
        let dir = tempdir().unwrap();
        let env_dir = dir.path().join(DESLICER_ENVIRONMENTS_DIR);
        std::fs::create_dir_all(&env_dir).unwrap();
        std::fs::write(env_dir.join("prod.yml"), "x\n").unwrap();
        std::fs::write(env_dir.join("staging.yml"), "x\n").unwrap();
        let err = resolve_environment_stem(None, None, &[dir.path()]).unwrap_err();
        assert!(err.to_string().contains("multiple environment files"));
        assert!(err.to_string().contains("DESLICER_ENVIRONMENT"));
    }

    #[test]
    fn optional_resolve_is_none_when_empty() {
        let dir = tempdir().unwrap();
        let resolved = resolve_optional_environment_stem(None, None, &[dir.path()]).unwrap();
        assert!(resolved.is_none());
    }
}
