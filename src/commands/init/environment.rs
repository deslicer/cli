//! Write or merge `.deslicer/environments/<tenant-slug>.yml` on Path A2 init.

use std::path::Path;

use crate::commands::pipeline::AuthenticatedSession;
use crate::environment_paths::{
    environment_file_on_disk, resolve_environment_stem, search_roots_for, ResolvedStem,
};
use crate::environment_yaml::merge_environment_yaml;
use crate::errors::CliError;
use crate::observer_client::{Client, HostGroup};
use crate::token_store::load_active_session;

use super::provider::InitProvider;

/// Path A2 / Observer API token: write the tenant environment file.
pub fn should_write_environment(
    provider: InitProvider,
    session: Option<&AuthenticatedSession>,
) -> bool {
    matches!(provider, InitProvider::GithubToken)
        || session.is_some_and(AuthenticatedSession::is_observer_api_token)
}

pub async fn write_tenant_environment(
    dir: &Path,
    client: &Client,
    explicit_environment: Option<&str>,
) -> Result<EnvironmentWrite, CliError> {
    let resolved = resolve_stem(dir, explicit_environment)?;
    let names = host_group_names(client).await?;
    let dest = environment_file_on_disk(dir, &resolved.stem);
    let existing = read_existing(&dest)?;
    let merged =
        merge_environment_yaml(&resolved.stem, &resolved.label, &names, existing.as_deref());
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| CliError::Other(format!("create {}: {err}", parent.display())))?;
    }
    std::fs::write(&dest, &merged.content)
        .map_err(|err| CliError::Other(format!("write {}: {err}", dest.display())))?;
    Ok(EnvironmentWrite {
        relative_path: merged.path,
        stem: resolved.stem,
    })
}

pub struct EnvironmentWrite {
    pub relative_path: String,
    pub stem: String,
}

pub fn print_environment_write(
    ctx: &crate::Ctx,
    written: &EnvironmentWrite,
    origin: Option<&super::provider::OriginRepo>,
) {
    let repo = super::github_env_recipe::origin_repo_slug(origin);
    match ctx.log_format {
        crate::cli::LogFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "wrote": written.relative_path,
                    "deslicer_environment": written.stem,
                    "github_environment": written.stem,
                })
            );
        }
        crate::cli::LogFormat::Human => {
            println!("wrote {}", written.relative_path);
            println!();
            println!(
                "{}",
                super::github_env_recipe::github_environment_recipe(&written.stem, repo.as_deref(),)
            );
        }
    }
}

fn resolve_stem(dir: &Path, explicit: Option<&str>) -> Result<ResolvedStem, CliError> {
    let tenant_slug = load_active_session()?.and_then(|session| session.tenant_slug);
    let roots = search_roots_for(dir);
    let refs: Vec<&Path> = roots.iter().map(|path| path.as_path()).collect();
    resolve_environment_stem(explicit, tenant_slug.as_deref(), &refs)
}

async fn host_group_names(client: &Client) -> Result<Vec<String>, CliError> {
    let groups = client.list_groups().await?;
    Ok(names_from_groups(&groups))
}

fn names_from_groups(groups: &[HostGroup]) -> Vec<String> {
    groups.iter().map(|group| group.name.clone()).collect()
}

fn read_existing(path: &Path) -> Result<Option<String>, CliError> {
    if !path.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(path)
        .map(Some)
        .map_err(|err| CliError::Other(format!("read {}: {err}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_token_always_writes() {
        assert!(should_write_environment(InitProvider::GithubToken, None));
        assert!(!should_write_environment(InitProvider::Github, None));
    }

    #[test]
    fn names_skip_nothing_from_host_groups() {
        let names = names_from_groups(&[HostGroup {
            id: "1".into(),
            name: "indexers".into(),
            display_name: None,
            member_count: None,
        }]);
        assert_eq!(names, vec!["indexers".to_string()]);
    }
}
