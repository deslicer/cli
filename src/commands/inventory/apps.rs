use clap::{Args as ClapArgs, Subcommand};
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::{
    flatten_installed, AdoptAppRequest, AppHostRef, Client, InsightsApp, InventoryApp,
};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Only apps installed on this host (hostname or host UUID). Exit 2 when
    /// no enrolled host matches.
    #[arg(long)]
    pub host: Option<String>,

    /// Only hosts in this Ansible inventory group (from `GET /api/v1/inventory`).
    /// Exit 2 when the group does not exist.
    #[arg(long)]
    pub group: Option<String>,

    #[arg(long)]
    pub environment: Option<String>,

    #[command(subcommand)]
    pub action: Option<AppsAction>,
}

#[derive(Subcommand)]
pub enum AppsAction {
    /// Adopt a live app as Git-managed for one host (dry-run unless --yes).
    ///
    /// Sends `management_mode=managed`, `content_source=git_tree`. Observer
    /// requires admin scope and a verified human session (device login).
    Adopt(AdoptArgs),
}

#[derive(ClapArgs)]
pub struct AdoptArgs {
    /// App name to adopt (as shown by `inventory apps`)
    pub app: String,

    /// Host where the app is installed (hostname or host UUID)
    #[arg(long)]
    pub host: String,

    /// Optional host group (name or UUID) to bind the provenance row to
    #[arg(long)]
    pub group: Option<String>,

    /// Splunk app scope directory: `apps` or `deployment-apps`
    #[arg(long, default_value = "apps", value_parser = ["apps", "deployment-apps"])]
    pub scope: String,

    /// Repo path of the app's Git source (e.g. `apps/TA-nix`)
    #[arg(long)]
    pub source_path: Option<String>,

    #[arg(long)]
    pub environment: Option<String>,

    /// Apply the adopt. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let format = ctx.log_format;
    let result = match args.action {
        Some(AppsAction::Adopt(adopt)) => adopt_run(&ctx, adopt).await,
        None => list_run(&ctx, args).await,
    };
    result.unwrap_or_else(|err| map_cli_error(format, err))
}

fn print_json(value: &impl serde::Serialize) -> i32 {
    crate::json_doc::print_success(value)
}

/// Every enrolled host Observer listed (installed or missing) for any app.
fn known_hosts(apps: &[InsightsApp]) -> impl Iterator<Item = &AppHostRef> {
    apps.iter()
        .flat_map(|a| a.installed_on.iter().chain(a.missing_from.iter()))
}

pub(crate) fn filter_rows(
    apps: &[InsightsApp],
    host: Option<&str>,
    group_hosts: Option<&[String]>,
) -> Result<Vec<InventoryApp>, CliError> {
    if let Some(needle) = host {
        if !known_hosts(apps).any(|h| h.matches(needle)) {
            return Err(CliError::InvalidInput(format!(
                "no enrolled host matches --host {needle:?}"
            )));
        }
    }
    Ok(flatten_installed(apps, &|h: &AppHostRef| {
        host.is_none_or(|n| h.matches(n))
            && group_hosts.is_none_or(|g| g.iter().any(|name| h.matches(name)))
    }))
}

async fn group_hostnames(client: &Client, group: &str) -> Result<Vec<String>, CliError> {
    let groups = client.list_inventory().await?;
    groups
        .into_iter()
        .find(|g| g.name == group)
        .map(|g| g.hosts)
        .ok_or_else(|| CliError::InvalidInput(format!("inventory group {group:?} not found")))
}

async fn list_run(ctx: &Ctx, args: Args) -> Result<i32, CliError> {
    let (_session, client) = authenticate(ctx, args.environment.as_deref(), None).await?;
    let apps = client.list_insights_apps().await?;
    let group_hosts = match args.group.as_deref() {
        Some(group) => Some(group_hostnames(&client, group).await?),
        None => None,
    };
    let rows = filter_rows(&apps, args.host.as_deref(), group_hosts.as_deref())?;
    Ok(match ctx.log_format {
        LogFormat::Json => print_json(&rows),
        LogFormat::Human => {
            print!("{}", format_apps_human(&rows));
            0
        }
    })
}

/// Resolve the host UUID for an app actually installed on `host`.
pub(crate) fn resolve_adopt_host(
    apps: &[InsightsApp],
    app: &str,
    host: &str,
) -> Result<AppHostRef, CliError> {
    let Some(row) = apps.iter().find(|a| a.app_name == app) else {
        return Err(CliError::InvalidInput(format!(
            "app {app:?} is not in the fleet apps inventory"
        )));
    };
    row.installed_on
        .iter()
        .find(|h| h.matches(host))
        .cloned()
        .ok_or_else(|| {
            CliError::InvalidInput(format!("app {app:?} is not installed on host {host:?}"))
        })
}

async fn adopt_run(ctx: &Ctx, args: AdoptArgs) -> Result<i32, CliError> {
    if !args.yes {
        return Ok(match ctx.log_format {
            LogFormat::Json => print_json(&json!({
                "dry_run": true,
                "action": "adopt",
                "app": args.app,
                "host": args.host,
                "group": args.group,
                "scope": args.scope,
                "management_mode": "managed",
                "content_source": "git_tree",
                "source_path": args.source_path,
            })),
            LogFormat::Human => {
                println!(
                    "dry-run: would adopt app {} on host {} as Git-managed (scope {}, group {})\npass --yes to apply",
                    args.app,
                    args.host,
                    args.scope,
                    args.group.as_deref().unwrap_or("-")
                );
                0
            }
        });
    }

    let (_session, client) = authenticate(ctx, args.environment.as_deref(), None).await?;
    let apps = client.list_insights_apps().await?;
    let host = resolve_adopt_host(&apps, &args.app, &args.host)?;
    let inventory_group_id = match args.group.as_deref() {
        Some(spec) => {
            let groups = client.list_groups().await?;
            Some(
                crate::target_group::resolve_target_group_id(spec, &groups)
                    .map_err(|e| CliError::InvalidInput(e.to_string()))?,
            )
        }
        None => None,
    };
    let body = AdoptAppRequest {
        scope: args.scope.clone(),
        app_name: args.app.clone(),
        host_id: host.host_id.clone(),
        inventory_group_id,
        management_mode: "managed".into(),
        content_source: "git_tree".into(),
        source_path: args.source_path.clone(),
    };
    let adopted = client.adopt_inventory_app(&body).await?;
    Ok(match ctx.log_format {
        LogFormat::Json => print_json(&adopted),
        LogFormat::Human => {
            println!(
                "adopted {} on {} as {} (scope {}, group {})",
                adopted.app_name,
                host.hostname.as_deref().unwrap_or(&host.host_id),
                adopted.management_mode,
                adopted.scope,
                adopted.inventory_group_id.as_deref().unwrap_or("-")
            );
            0
        }
    })
}

fn format_apps_human(apps: &[InventoryApp]) -> String {
    if apps.is_empty() {
        return "No apps found for the given filters.\n".to_string();
    }
    let mut lines = vec!["NAME  VERSION  HOST  LAST_SEEN".to_string()];
    for app in apps {
        lines.push(format!(
            "{}  {}  {}  {}",
            app.name,
            app.version.as_deref().unwrap_or("-"),
            app.host,
            app.last_seen.as_deref().unwrap_or("-")
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn host(id: &str, name: &str) -> AppHostRef {
        AppHostRef {
            host_id: id.into(),
            hostname: Some(name.into()),
            observed_version: None,
        }
    }

    fn fleet() -> Vec<InsightsApp> {
        vec![InsightsApp {
            app_name: "TA-nix".into(),
            installed_on: vec![host("h1", "idx1")],
            missing_from: vec![host("h2", "sh1")],
            observed_version: Some("9.0.0".into()),
            last_seen: None,
        }]
    }

    #[test]
    fn human_list_rows() {
        let rows = filter_rows(&fleet(), None, None).unwrap();
        let text = format_apps_human(&rows);
        assert!(text.contains("TA-nix  9.0.0  idx1"));
    }

    #[test]
    fn host_filter_known_host_without_app_is_empty_not_error() {
        assert!(filter_rows(&fleet(), Some("sh1"), None).unwrap().is_empty());
    }

    #[test]
    fn unknown_host_is_invalid_input() {
        let err = filter_rows(&fleet(), Some("nope"), None).unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }

    #[test]
    fn group_filter_uses_inventory_hostnames() {
        let g = vec!["sh1".to_string()];
        assert!(filter_rows(&fleet(), None, Some(&g)).unwrap().is_empty());
        let g = vec!["idx1".to_string()];
        assert_eq!(filter_rows(&fleet(), None, Some(&g)).unwrap().len(), 1);
    }

    #[test]
    fn adopt_requires_app_installed_on_host() {
        assert_eq!(
            resolve_adopt_host(&fleet(), "TA-nix", "IDX1")
                .unwrap()
                .host_id,
            "h1"
        );
        assert!(matches!(
            resolve_adopt_host(&fleet(), "TA-nix", "sh1"),
            Err(CliError::InvalidInput(_))
        ));
        assert!(matches!(
            resolve_adopt_host(&fleet(), "missing", "idx1"),
            Err(CliError::InvalidInput(_))
        ));
    }

    #[test]
    fn help_exposes_apps_and_adopt() {
        let mut cmd = crate::cli::Cli::command();
        let inventory = cmd.find_subcommand_mut("inventory").expect("inventory");
        let apps = inventory.find_subcommand_mut("apps").expect("apps");
        let mut buf = Vec::new();
        apps.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--host"));
        assert!(help.contains("adopt"));
        let adopt = apps.find_subcommand_mut("adopt").expect("adopt");
        let mut buf = Vec::new();
        adopt.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--yes"));
        assert!(help.contains("--host"));
    }
}
