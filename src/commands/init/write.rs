use std::fs;
use std::path::Path;

use crate::cli::LogFormat;
use crate::errors::CliError;
use crate::Ctx;

use super::provider::InitProvider;
use super::templates::{resolve_write_path, DecodedTemplate};

pub fn write_templates(
    dir: &Path,
    provider: InitProvider,
    files: &[DecodedTemplate],
    force: bool,
) -> Result<WriteSummary, CliError> {
    let root = dir
        .canonicalize()
        .map_err(|err| CliError::Other(format!("cannot resolve --dir {}: {err}", dir.display())))?;
    let mut existing = Vec::new();
    for file in files {
        let dest = resolve_write_path(&root, &file.path)?;
        if dest.exists() && !force && !file.if_missing {
            existing.push(file.path.clone());
        }
    }
    if !existing.is_empty() {
        return Err(CliError::Other(format!(
            "refusing to overwrite existing {} files (pass --force): {}",
            provider.as_str(),
            existing.join(", ")
        )));
    }

    let mut written = 0usize;
    let mut skipped = 0usize;
    for file in files {
        let dest = resolve_write_path(&root, &file.path)?;
        if dest.exists() && file.if_missing && !force {
            skipped += 1;
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| CliError::Other(format!("create {} parent: {err}", file.path)))?;
        }
        let dest = resolve_write_path(&root, &file.path)?;
        fs::write(&dest, &file.contents)
            .map_err(|err| CliError::Other(format!("write {}: {err}", file.path)))?;
        written += 1;
    }
    Ok(WriteSummary { written, skipped })
}

#[derive(Debug, Clone, Copy)]
pub struct WriteSummary {
    pub written: usize,
    pub skipped: usize,
}

pub fn print_write_summary(
    ctx: &Ctx,
    provider: InitProvider,
    dir: &Path,
    written: usize,
    skipped: usize,
) {
    match ctx.log_format {
        LogFormat::Json => {
            let payload = serde_json::json!({
                "provider": provider.as_str(),
                "dir": dir.display().to_string(),
                "written": written,
                "skipped": skipped,
            });
            println!("{payload}");
        }
        LogFormat::Human => {
            println!(
                "Wrote {written} {} file(s) under {}.",
                provider.as_str(),
                dir.display()
            );
            if skipped > 0 {
                println!("Skipped {skipped} existing README file(s) (IfMissing).");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn file(path: &str, contents: &[u8], if_missing: bool) -> DecodedTemplate {
        DecodedTemplate {
            path: path.to_string(),
            contents: contents.to_vec(),
            if_missing,
        }
    }

    #[test]
    fn refuses_overwrite_without_force() {
        let dir = tempdir().unwrap();
        let workflow = dir.path().join(".github/workflows");
        fs::create_dir_all(&workflow).unwrap();
        fs::write(workflow.join("deslicer-plan.yml"), b"old").unwrap();
        let err = write_templates(
            dir.path(),
            InitProvider::Github,
            &[file(".github/workflows/deslicer-plan.yml", b"new", false)],
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("--force"));
    }

    #[test]
    fn skips_existing_readme_unless_force() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("README.md"), b"mine").unwrap();
        let summary = write_templates(
            dir.path(),
            InitProvider::Github,
            &[file("README.md", b"generated", true)],
            false,
        )
        .unwrap();
        assert_eq!(summary.skipped, 1);
        assert_eq!(fs::read(dir.path().join("README.md")).unwrap(), b"mine");
    }
}
