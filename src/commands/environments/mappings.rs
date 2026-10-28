use clap::{Args as ClapArgs, Subcommand};
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::EnvironmentAppMapping;
use crate::Ctx;

#[derive(Subcommand)]
pub enum MappingsCmd {
    /// List environment app mappings (Observer source of truth)
    List(ListArgs),
    /// Set / upsert one app mapping (dry-run unless --yes)
    Set(SetArgs),
    /// Adopt a live app into the environment mapping (dry-run unless --yes)
    Adopt(AdoptArgs),
}

#[derive(ClapArgs)]
pub struct ListArgs {
    /// Environment name (required; Observer SoT is per environment)
    #[arg(long)]
    pub environment: String,
}

#[derive(ClapArgs)]
pub struct SetArgs {
    #[arg(long)]
    pub environment: String,

    /// App name to map
    #[arg(long)]
    pub app: String,

    /// Repo-relative source path for the app
    #[arg(long)]
    pub source_path: String,

    /// Inventory / host group name
    #[arg(long)]
    pub group: Option<String>,

    /// Optional host scope
    #[arg(long)]
    pub host: Option<String>,

    /// Apply the mapping. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

#[derive(ClapArgs)]
pub struct AdoptArgs {
    #[arg(long)]
    pub environment: String,

    /// App name discovered on a host / in inventory
    #[arg(long)]
    pub app: String,

    #[arg(long)]
    pub host: Option<String>,

    #[arg(long)]
    pub group: Option<String>,

    /// Apply the adopt. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn dispatch(ctx: Ctx, cmd: MappingsCmd) -> i32 {
    match cmd {
        MappingsCmd::List(args) => list(ctx, args).await,
        MappingsCmd::Set(args) => set(ctx, args).await,
        MappingsCmd::Adopt(args) => adopt(ctx, args).await,
    }
}

async fn list(ctx: Ctx, args: ListArgs) -> i32 {
    let (_session, client) = match authenticate(&ctx, Some(&args.environment), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let mappings = match client.list_environment_mappings(&args.environment).await {
        Ok(mappings) => mappings,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&mappings) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize mappings: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_mappings_human(&mappings));
            0
        }
    }
}

async fn set(ctx: Ctx, args: SetArgs) -> i32 {
    if !args.yes {
        print_set_dry_run(&ctx, &args);
        return 0;
    }
    let (_session, client) = match authenticate(&ctx, Some(&args.environment), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let mapping = EnvironmentAppMapping {
        app: args.app.clone(),
        source_path: Some(args.source_path.clone()),
        inventory_group: args.group.clone(),
        host: args.host.clone(),
        status: None,
    };
    match client
        .set_environment_mapping(&args.environment, &mapping)
        .await
    {
        Ok(result) => emit_mapping(&ctx, &result),
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

async fn adopt(ctx: Ctx, args: AdoptArgs) -> i32 {
    if !args.yes {
        print_adopt_dry_run(&ctx, &args);
        return 0;
    }
    let (_session, client) = match authenticate(&ctx, Some(&args.environment), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client
        .adopt_environment_mapping(
            &args.environment,
            &args.app,
            args.host.as_deref(),
            args.group.as_deref(),
        )
        .await
    {
        Ok(result) => emit_mapping(&ctx, &result),
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn emit_mapping(ctx: &Ctx, mapping: &EnvironmentAppMapping) -> i32 {
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(mapping) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize mapping: {err}");
                1
            }
        },
        LogFormat::Human => {
            println!(
                "mapping ok: app={} source_path={} group={} host={}",
                mapping.app,
                mapping.source_path.as_deref().unwrap_or("-"),
                mapping.inventory_group.as_deref().unwrap_or("-"),
                mapping.host.as_deref().unwrap_or("-")
            );
            0
        }
    }
}

fn print_set_dry_run(ctx: &Ctx, args: &SetArgs) {
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            json!({
                "dry_run": true,
                "action": "set",
                "environment": args.environment,
                "app": args.app,
                "source_path": args.source_path,
                "group": args.group,
                "host": args.host,
            })
        ),
        LogFormat::Human => print!("{}", set_dry_run_text(args)),
    }
}

fn print_adopt_dry_run(ctx: &Ctx, args: &AdoptArgs) {
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            json!({
                "dry_run": true,
                "action": "adopt",
                "environment": args.environment,
                "app": args.app,
                "host": args.host,
                "group": args.group,
            })
        ),
        LogFormat::Human => print!("{}", adopt_dry_run_text(args)),
    }
}

fn set_dry_run_text(args: &SetArgs) -> String {
    format!(
        "dry-run: would set mapping app={} source_path={} group={} host={} in environment {}\n\
         pass --yes to apply\n",
        args.app,
        args.source_path,
        args.group.as_deref().unwrap_or("-"),
        args.host.as_deref().unwrap_or("-"),
        args.environment
    )
}

fn adopt_dry_run_text(args: &AdoptArgs) -> String {
    format!(
        "dry-run: would adopt app={} host={} group={} into environment {}\n\
         pass --yes to apply\n",
        args.app,
        args.host.as_deref().unwrap_or("-"),
        args.group.as_deref().unwrap_or("-"),
        args.environment
    )
}

fn format_mappings_human(mappings: &[EnvironmentAppMapping]) -> String {
    if mappings.is_empty() {
        return "No environment app mappings.\n".to_string();
    }
    let mut lines = vec!["APP  SOURCE_PATH  GROUP  HOST  STATUS".to_string()];
    for m in mappings {
        lines.push(format!(
            "{}  {}  {}  {}  {}",
            m.app,
            m.source_path.as_deref().unwrap_or("-"),
            m.inventory_group.as_deref().unwrap_or("-"),
            m.host.as_deref().unwrap_or("-"),
            m.status.as_deref().unwrap_or("-")
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
    fn set_dry_run_mentions_yes() {
        let text = set_dry_run_text(&SetArgs {
            environment: "prod".into(),
            app: "ta_nix".into(),
            source_path: "apps/ta_nix".into(),
            group: Some("indexers".into()),
            host: None,
            yes: false,
        });
        assert!(text.contains("--yes"));
        assert!(text.contains("ta_nix"));
    }

    #[test]
    fn help_requires_yes_for_set_and_adopt() {
        let mut cmd = crate::cli::Cli::command();
        let env = cmd
            .find_subcommand_mut("environments")
            .expect("environments");
        let mappings = env.find_subcommand_mut("mappings").expect("mappings");
        for name in ["set", "adopt"] {
            let sub = mappings.find_subcommand_mut(name).expect(name);
            let mut buf = Vec::new();
            sub.write_long_help(&mut buf).expect("help");
            let help = String::from_utf8(buf).expect("utf8");
            assert!(help.contains("--yes"), "{name} help missing --yes");
        }
    }
}
