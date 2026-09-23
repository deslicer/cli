use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;
use serde::Serialize;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::environment_paths::{environment_file_on_disk, list_environment_stems};
use crate::environment_yaml::DESLICER_ENVIRONMENTS_DIR;
use crate::errors::CliError;
use crate::target_group::inventory_groups_with_apps;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Limit discovery to one environment filename stem
    #[arg(long)]
    pub environment: Option<String>,

    /// Repository root containing `.deslicer/environments`
    #[arg(long, default_value = ".")]
    pub repo_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PlanMatrixEntry {
    environment: String,
    inventory_group: String,
}

#[derive(Debug, Serialize)]
struct PlanMatrixOutput {
    include: Vec<PlanMatrixEntry>,
}

pub fn run(ctx: Ctx, args: Args) -> i32 {
    let entries = match PlanMatrixManager::new(args.repo_root).discover(args.environment.as_deref())
    {
        Ok(entries) => entries,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let output = PlanMatrixOutput { include: entries };

    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string(&output) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize plan matrix: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_human(&output.include));
            0
        }
    }
}

struct PlanMatrixManager {
    repo_root: PathBuf,
}

impl PlanMatrixManager {
    fn new(repo_root: PathBuf) -> Self {
        Self { repo_root }
    }

    fn discover(&self, environment: Option<&str>) -> Result<Vec<PlanMatrixEntry>, CliError> {
        let mut stems = match environment.map(str::trim).filter(|value| !value.is_empty()) {
            Some(stem) => vec![stem.to_string()],
            None => list_environment_stems(&[self.repo_root.as_path()])?,
        };
        stems.sort_unstable();

        let mut entries = Vec::new();
        for stem in stems {
            let yaml = self.read_environment(&stem)?;
            entries.extend(
                inventory_groups_with_apps(&yaml)
                    .into_iter()
                    .map(|inventory_group| PlanMatrixEntry {
                        environment: stem.clone(),
                        inventory_group,
                    }),
            );
        }
        Ok(entries)
    }

    fn read_environment(&self, stem: &str) -> Result<String, CliError> {
        let path = resolve_environment_path(&self.repo_root, stem).ok_or_else(|| {
            CliError::Other(format!(
                "environment file not found for {stem:?} under {DESLICER_ENVIRONMENTS_DIR}"
            ))
        })?;
        std::fs::read_to_string(&path)
            .map_err(|err| CliError::Other(format!("read {}: {err}", path.display())))
    }
}

fn resolve_environment_path(repo_root: &Path, stem: &str) -> Option<PathBuf> {
    let yml = environment_file_on_disk(repo_root, stem);
    if yml.is_file() {
        return Some(yml);
    }
    let yaml = repo_root
        .join(DESLICER_ENVIRONMENTS_DIR)
        .join(format!("{stem}.yaml"));
    yaml.is_file().then_some(yaml)
}

fn format_human(entries: &[PlanMatrixEntry]) -> String {
    if entries.is_empty() {
        return "No environment destinations contain apps; CI can skip plan jobs.\n".into();
    }
    let mut lines = vec![format!("Plan matrix ({} jobs):", entries.len())];
    for entry in entries {
        lines.push(format!(
            "  {} / {}",
            entry.environment, entry.inventory_group
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_only_destinations_with_apps() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(DESLICER_ENVIRONMENTS_DIR);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("prod.yml"),
            "destinations:\n  - inventory_group: indexers\n    apps:\n      - source_path: apps/a\n  - inventory_group: empty\n    apps: []\n",
        )
        .unwrap();

        let entries = PlanMatrixManager::new(root.path().to_path_buf())
            .discover(None)
            .unwrap();

        assert_eq!(
            entries,
            vec![PlanMatrixEntry {
                environment: "prod".into(),
                inventory_group: "indexers".into(),
            }]
        );
    }

    #[test]
    fn explicit_missing_environment_fails() {
        let root = tempfile::tempdir().unwrap();
        let error = PlanMatrixManager::new(root.path().to_path_buf())
            .discover(Some("missing"))
            .unwrap_err();
        assert!(error.to_string().contains("environment file not found"));
    }
}
