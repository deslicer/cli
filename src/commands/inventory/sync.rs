//! `deslicer inventory sync` — refresh the tenant environment file.

use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::environment_paths::{
    environment_file_on_disk, resolve_environment_stem, search_roots_for, ResolvedStem,
};
use crate::environment_yaml::{merge_environment_yaml, BlockedDestination, MergedEnvironmentYaml};
use crate::errors::CliError;
use crate::observer_client::Client;
use crate::token_store::load_active_session;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,

    /// Repository root that contains `.deslicer/environments/` (default: `.`).
    #[arg(long)]
    pub dir: Option<PathBuf>,

    /// Print planned adds / removes without writing.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(&ctx, args).await {
        Ok(blocked) if blocked => 2,
        Ok(_) => 0,
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

async fn run_inner(ctx: &Ctx, args: Args) -> Result<bool, CliError> {
    let dir = resolve_dir(args.dir.as_deref())?;
    let (_session, client) = authenticate(ctx, args.environment.as_deref(), None).await?;
    let resolved = resolve_stem(&dir, args.environment.as_deref())?;
    let merged = fetch_and_merge(&client, &dir, &resolved).await?;
    emit_report(ctx, &merged, args.dry_run);
    if !args.dry_run {
        write_merged(&dir, &resolved.stem, &merged.content)?;
    }
    Ok(!merged.blocked.is_empty())
}

fn resolve_dir(dir: Option<&Path>) -> Result<PathBuf, CliError> {
    let dir = dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    if !dir.is_dir() {
        return Err(CliError::Other(format!(
            "--dir {} is not a directory",
            dir.display()
        )));
    }
    Ok(dir)
}

fn resolve_stem(dir: &Path, explicit: Option<&str>) -> Result<ResolvedStem, CliError> {
    let tenant_slug = load_active_session()?.and_then(|session| session.tenant_slug);
    let roots = search_roots_for(dir);
    let refs: Vec<&Path> = roots.iter().map(|path| path.as_path()).collect();
    resolve_environment_stem(explicit, tenant_slug.as_deref(), &refs)
}

async fn fetch_and_merge(
    client: &Client,
    dir: &Path,
    resolved: &ResolvedStem,
) -> Result<MergedEnvironmentYaml, CliError> {
    let groups = client.list_groups().await?;
    let names: Vec<String> = groups.into_iter().map(|group| group.name).collect();
    let dest = environment_file_on_disk(dir, &resolved.stem);
    let existing = if dest.exists() {
        Some(
            std::fs::read_to_string(&dest)
                .map_err(|err| CliError::Other(format!("read {}: {err}", dest.display())))?,
        )
    } else {
        None
    };
    Ok(merge_environment_yaml(
        &resolved.stem,
        &resolved.label,
        &names,
        existing.as_deref(),
    ))
}

fn write_merged(dir: &Path, stem: &str, content: &str) -> Result<(), CliError> {
    let dest = environment_file_on_disk(dir, stem);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| CliError::Other(format!("create {}: {err}", parent.display())))?;
    }
    std::fs::write(&dest, content)
        .map_err(|err| CliError::Other(format!("write {}: {err}", dest.display())))
}

fn emit_report(ctx: &Ctx, merged: &MergedEnvironmentYaml, dry_run: bool) {
    match ctx.log_format {
        LogFormat::Json => println!("{}", json_report(merged, dry_run)),
        LogFormat::Human => print!("{}", human_report(merged, dry_run)),
    }
}

fn json_report(merged: &MergedEnvironmentYaml, dry_run: bool) -> serde_json::Value {
    serde_json::json!({
        "path": merged.path,
        "dry_run": dry_run,
        "added": merged.added,
        "removed_empty": merged.removed_empty,
        "blocked": merged.blocked.iter().map(|dest| {
            serde_json::json!({
                "inventory_group": dest.inventory_group,
                "apps": dest.source_paths,
            })
        }).collect::<Vec<_>>(),
    })
}

fn human_report(merged: &MergedEnvironmentYaml, dry_run: bool) -> String {
    let mut lines = Vec::new();
    let prefix = if dry_run {
        "inventory sync (dry-run)"
    } else {
        "inventory sync"
    };
    for name in &merged.added {
        lines.push(format!("{prefix}: added {name}"));
    }
    for name in &merged.removed_empty {
        lines.push(format!("{prefix}: removed empty group {name}"));
    }
    if merged.blocked.is_empty() && merged.added.is_empty() && merged.removed_empty.is_empty() {
        lines.push(format!("{prefix}: no changes"));
    }
    for dest in &merged.blocked {
        lines.push(blocked_human(dest));
    }
    if !merged.blocked.is_empty() {
        lines.push(
            "Delete those apps from the environment file (and the repo) first, then re-run `deslicer inventory sync`."
                .into(),
        );
    }
    if !dry_run {
        lines.push(format!("wrote {}", merged.path));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn blocked_human(dest: &BlockedDestination) -> String {
    let mut out = format!(
        "inventory sync: cannot remove {} — apps still listed:",
        dest.inventory_group
    );
    for path in &dest.source_paths {
        out.push_str(&format!("\n  - {path}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_blocked() -> MergedEnvironmentYaml {
        MergedEnvironmentYaml {
            path: ".deslicer/environments/acme-prod.yml".into(),
            content: String::new(),
            host_group_placeholder: false,
            added: vec!["search_heads".into()],
            removed_empty: vec!["legacy_empty".into()],
            blocked: vec![BlockedDestination {
                inventory_group: "indexers".into(),
                source_paths: vec!["apps/ta_nix".into(), "apps/ta_windows".into()],
            }],
        }
    }

    #[test]
    fn human_report_lists_blocked_apps() {
        let text = human_report(&sample_blocked(), false);
        assert!(text.contains("inventory sync: added search_heads"));
        assert!(text.contains("inventory sync: removed empty group legacy_empty"));
        assert!(text.contains("cannot remove indexers"));
        assert!(text.contains("  - apps/ta_nix"));
        assert!(text.contains("  - apps/ta_windows"));
        assert!(text.contains("re-run `deslicer inventory sync`"));
        assert!(text.contains("wrote .deslicer/environments/acme-prod.yml"));
    }

    #[test]
    fn json_report_includes_blocked_apps() {
        let value = json_report(&sample_blocked(), true);
        assert_eq!(value["added"][0], "search_heads");
        assert_eq!(value["removed_empty"][0], "legacy_empty");
        assert_eq!(value["blocked"][0]["inventory_group"], "indexers");
        assert_eq!(value["blocked"][0]["apps"][0], "apps/ta_nix");
        assert_eq!(value["dry_run"], true);
    }
}
