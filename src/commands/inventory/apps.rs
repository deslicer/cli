use clap::{Args as ClapArgs, Subcommand};
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::InventoryApp;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Filter by host name
    #[arg(long)]
    pub host: Option<String>,

    /// Filter by inventory / host group name
    #[arg(long)]
    pub group: Option<String>,

    #[arg(long)]
    pub environment: Option<String>,

    #[command(subcommand)]
    pub action: Option<AppsAction>,
}

#[derive(Subcommand)]
pub enum AppsAction {
    /// Adopt a live app into provenance for a host (dry-run unless --yes)
    Adopt(AdoptArgs),
}

#[derive(ClapArgs)]
pub struct AdoptArgs {
    /// App name to adopt
    pub app: String,

    /// Host where the app was discovered
    #[arg(long)]
    pub host: String,

    /// Optional inventory group
    #[arg(long)]
    pub group: Option<String>,

    #[arg(long)]
    pub environment: Option<String>,

    /// Apply the adopt. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match args.action {
        Some(AppsAction::Adopt(adopt)) => adopt_run(ctx, adopt).await,
        None => list_run(ctx, args).await,
    }
}

async fn list_run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let apps = match client
        .list_inventory_apps(args.host.as_deref(), args.group.as_deref())
        .await
    {
        Ok(apps) => apps,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&apps) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize apps: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_apps_human(&apps));
            0
        }
    }
}

async fn adopt_run(ctx: Ctx, args: AdoptArgs) -> i32 {
    if !args.yes {
        match ctx.log_format {
            LogFormat::Json => println!(
                "{}",
                json!({
                    "dry_run": true,
                    "action": "adopt",
                    "app": args.app,
                    "host": args.host,
                    "group": args.group,
                })
            ),
            LogFormat::Human => println!(
                "dry-run: would adopt app {} on host {} (group {})\npass --yes to apply",
                args.app,
                args.host,
                args.group.as_deref().unwrap_or("-")
            ),
        }
        return 0;
    }

    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client
        .adopt_inventory_app(&args.app, &args.host, args.group.as_deref())
        .await
    {
        Ok(app) => match ctx.log_format {
            LogFormat::Json => match serde_json::to_string_pretty(&app) {
                Ok(json) => {
                    println!("{json}");
                    0
                }
                Err(err) => {
                    eprintln!("failed to serialize adopt result: {err}");
                    1
                }
            },
            LogFormat::Human => {
                println!(
                    "adopted {} on {} (group {})",
                    app.name.as_deref().unwrap_or(&args.app),
                    app.host.as_deref().unwrap_or(&args.host),
                    app.inventory_group
                        .as_deref()
                        .or(args.group.as_deref())
                        .unwrap_or("-")
                );
                0
            }
        },
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn format_apps_human(apps: &[InventoryApp]) -> String {
    if apps.is_empty() {
        return "No apps found for the given filters.\n".to_string();
    }
    let mut lines = vec!["NAME  VERSION  HOST  GROUP  STATUS  SOURCE_PATH".to_string()];
    for app in apps {
        lines.push(format!(
            "{}  {}  {}  {}  {}  {}",
            app.name.as_deref().unwrap_or("-"),
            app.version.as_deref().unwrap_or("-"),
            app.host.as_deref().unwrap_or("-"),
            app.inventory_group.as_deref().unwrap_or("-"),
            app.status.as_deref().unwrap_or("-"),
            app.source_path.as_deref().unwrap_or("-")
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn human_list_rows() {
        let text = format_apps_human(&[InventoryApp {
            name: Some("ta_nix".into()),
            version: Some("1.0".into()),
            host: Some("idx1".into()),
            inventory_group: Some("indexers".into()),
            status: Some("discovered".into()),
            source_path: None,
        }]);
        assert!(text.contains("ta_nix"));
        assert!(text.contains("idx1"));
    }

    #[test]
    fn human_list_tolerates_missing_name() {
        let text = format_apps_human(&[InventoryApp {
            name: None,
            version: None,
            host: Some("idx1".into()),
            inventory_group: None,
            status: None,
            source_path: None,
        }]);
        assert!(text.contains("-  -  idx1"));
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
