//! Build the ACS project-plan body from `.deslicer/environments/<env>.yml`
//! plus the app directories it references (`source_path`).
//!
//! Every failure here is operator input (exit 2): the CLI never sends an
//! empty `apps` list.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::environment_yaml::extract_apps_blocks;
use crate::errors::CliError;
use crate::observer_client::{AcsProjectPlanApp, AcsProjectPlanRequest};

/// Inventory group name Observer uses for Cloud stacks when none is given.
const CLOUD_GROUP: &str = "cloud";
/// Guard against packing a whole repo by mistake.
const MAX_FILES_PER_APP: usize = 2_000;
const MAX_BYTES_PER_APP: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub struct AcsPlanInputs {
    pub environment_file: PathBuf,
    pub inventory_group: String,
    pub request: AcsProjectPlanRequest,
}

/// Resolve inputs for `--destination acs-stack:<stack>`.
///
/// Group selection: an `inventory_group` equal to the stack name wins, then
/// `cloud`, then the only group with apps. Anything else is ambiguous.
pub fn resolve_acs_plan_inputs(
    stack: &str,
    environment: Option<&str>,
    roots: &[PathBuf],
) -> Result<AcsPlanInputs, CliError> {
    let Some(stem) = environment.map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(CliError::InvalidInput(
            "--destination acs-stack: needs an environment: pass --environment <name> \
             (reads .deslicer/environments/<name>.yml)"
                .into(),
        ));
    };
    let Some((root, file)) = find_env_file(roots, stem) else {
        return Err(CliError::InvalidInput(format!(
            "environment {stem:?} not found: expected .deslicer/environments/{stem}.yml \
             under {}",
            roots
                .iter()
                .map(|r| r.display().to_string())
                .collect::<Vec<_>>()
                .join(" or ")
        )));
    };
    let content = std::fs::read_to_string(&file)
        .map_err(|e| CliError::InvalidInput(format!("read {}: {e}", file.display())))?;

    let blocks: Vec<_> = extract_apps_blocks(&content)
        .into_iter()
        .filter(|b| !b.source_paths().is_empty())
        .collect();
    let pick = |name: &str| blocks.iter().find(|b| b.group_name == name);
    let block = match pick(stack).or_else(|| pick(CLOUD_GROUP)) {
        Some(b) => b,
        None if blocks.len() == 1 => &blocks[0],
        None if blocks.is_empty() => {
            return Err(CliError::InvalidInput(format!(
                "{} has no destination with source_path apps; add an `inventory_group: \
                 {stack}` (or `cloud`) destination with apps",
                file.display()
            )))
        }
        None => {
            return Err(CliError::InvalidInput(format!(
                "{} maps apps to several inventory_groups ({}); name one `{stack}` or \
                 `cloud` for the ACS stack",
                file.display(),
                blocks
                    .iter()
                    .map(|b| b.group_name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
    };

    let mut apps = Vec::new();
    for source_path in block.source_paths() {
        apps.push(load_app(&root, &source_path, &block.group_name)?);
    }
    if apps.is_empty() {
        return Err(CliError::InvalidInput(format!(
            "no apps resolved for inventory_group {:?} in {}",
            block.group_name,
            file.display()
        )));
    }

    Ok(AcsPlanInputs {
        environment_file: file,
        inventory_group: block.group_name.clone(),
        request: AcsProjectPlanRequest {
            environment_name: stem.to_string(),
            mapping_source: "repo_yaml".into(),
            apps,
        },
    })
}

fn find_env_file(roots: &[PathBuf], stem: &str) -> Option<(PathBuf, PathBuf)> {
    for root in roots {
        for ext in ["yml", "yaml"] {
            let path = root
                .join(crate::environment_yaml::DESLICER_ENVIRONMENTS_DIR)
                .join(format!("{stem}.{ext}"));
            if path.is_file() {
                return Some((root.clone(), path));
            }
        }
    }
    None
}

fn load_app(root: &Path, source_path: &str, group: &str) -> Result<AcsProjectPlanApp, CliError> {
    let rel = source_path.trim().trim_end_matches('/');
    if rel.is_empty() || Path::new(rel).is_absolute() || rel.split('/').any(|p| p == "..") {
        return Err(CliError::InvalidInput(format!(
            "source_path {source_path:?} must be a relative path inside the repo"
        )));
    }
    let dir = root.join(rel);
    if !dir.is_dir() {
        return Err(CliError::InvalidInput(format!(
            "source_path {rel:?} is not a directory ({})",
            dir.display()
        )));
    }
    let mut files = BTreeMap::new();
    let mut bytes = 0u64;
    collect_files(&dir, &dir, &mut files, &mut bytes)?;
    if files.is_empty() {
        return Err(CliError::InvalidInput(format!(
            "app {rel:?} has no text files to project"
        )));
    }
    let app_name = Path::new(rel)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel.to_string());
    Ok(AcsProjectPlanApp {
        app_name,
        source_path: rel.to_string(),
        inventory_group: group.to_string(),
        files,
    })
}

fn collect_files(
    base: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, String>,
    bytes: &mut u64,
) -> Result<(), CliError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| CliError::InvalidInput(format!("read {}: {e}", dir.display())))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| CliError::InvalidInput(format!("stat {}: {e}", path.display())))?;
        if kind.is_dir() {
            collect_files(base, &path, out, bytes)?;
        } else if kind.is_file() {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue; // binary assets (icons, tarballs) are not ACS config
            };
            *bytes += text.len() as u64;
            if out.len() >= MAX_FILES_PER_APP || *bytes > MAX_BYTES_PER_APP {
                return Err(CliError::InvalidInput(format!(
                    "app {} is too large to project (>{MAX_FILES_PER_APP} files or \
                     >{MAX_BYTES_PER_APP} bytes)",
                    base.display()
                )));
            }
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.insert(rel, text);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    const ENV: &str =
        "destinations:\n  - inventory_group: cloud\n    apps:\n      - source_path: apps/idx\n";

    #[test]
    fn builds_apps_from_env_yaml_and_bundle_files() {
        let dir = tempdir().unwrap();
        write(dir.path(), ".deslicer/environments/prod.yml", ENV);
        write(dir.path(), "apps/idx/default/indexes.conf", "[main]\n");
        write(dir.path(), "apps/idx/.git/HEAD", "x");
        let got = resolve_acs_plan_inputs("scv", Some("prod"), &[dir.path().into()]).unwrap();
        assert_eq!(got.request.environment_name, "prod");
        assert_eq!(got.inventory_group, "cloud");
        assert_eq!(got.request.apps.len(), 1);
        let app = &got.request.apps[0];
        assert_eq!(app.app_name, "idx");
        assert_eq!(
            app.files.keys().collect::<Vec<_>>(),
            vec!["default/indexes.conf"]
        );
        assert!(got
            .environment_file
            .ends_with(".deslicer/environments/prod.yml"));
    }

    #[test]
    fn missing_environment_is_invalid_input() {
        let dir = tempdir().unwrap();
        let err = resolve_acs_plan_inputs("scv", None, &[dir.path().into()]).unwrap_err();
        assert_eq!(err.exit_code(), 2);
        let err = resolve_acs_plan_inputs("scv", Some("nope"), &[dir.path().into()]).unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("nope"));
    }

    #[test]
    fn never_returns_empty_apps() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            ".deslicer/environments/prod.yml",
            "destinations:\n  - inventory_group: cloud\n    apps: []\n",
        );
        let err = resolve_acs_plan_inputs("scv", Some("prod"), &[dir.path().into()]).unwrap_err();
        assert_eq!(err.exit_code(), 2);

        write(dir.path(), ".deslicer/environments/prod.yml", ENV);
        std::fs::create_dir_all(dir.path().join("apps/idx")).unwrap();
        let err = resolve_acs_plan_inputs("scv", Some("prod"), &[dir.path().into()]).unwrap_err();
        assert!(err.to_string().contains("no text files"), "{err}");
    }

    #[test]
    fn ambiguous_groups_and_escaping_paths_are_rejected() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            ".deslicer/environments/prod.yml",
            "destinations:\n  - inventory_group: a\n    apps:\n      - source_path: apps/a\n  - inventory_group: b\n    apps:\n      - source_path: apps/b\n",
        );
        let err = resolve_acs_plan_inputs("scv", Some("prod"), &[dir.path().into()]).unwrap_err();
        assert!(err.to_string().contains("a, b"), "{err}");

        write(
            dir.path(),
            ".deslicer/environments/prod.yml",
            "destinations:\n  - inventory_group: scv\n    apps:\n      - source_path: ../etc\n",
        );
        let err = resolve_acs_plan_inputs("scv", Some("prod"), &[dir.path().into()]).unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }
}
