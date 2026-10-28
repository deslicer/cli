use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::output::emit_change_plans;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Catalog blueprint id (from `catalog list`)
    pub id: String,

    /// Target inventory groups (comma-separated), e.g. indexers,search_heads
    #[arg(long, value_delimiter = ',', num_args = 1.., required = true)]
    pub targets: Vec<String>,

    /// Optional plan name shown in Deslicer Changes/Runs
    #[arg(long)]
    pub name: Option<String>,

    #[arg(long)]
    pub environment: Option<String>,

    /// Create Pending plan(s). Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    if !args.yes {
        match ctx.log_format {
            LogFormat::Json => println!(
                "{}",
                json!({
                    "dry_run": true,
                    "action": "catalog_plan",
                    "catalog_id": args.id,
                    "targets": args.targets,
                    "will_auto_approve": false,
                })
            ),
            LogFormat::Human => println!(
                "dry-run: would create Pending plan(s) from catalog {} for targets [{}]\n\
                 plans are never auto-approved; pass --yes to create, then `deslicer change approve`",
                args.id,
                args.targets.join(", ")
            ),
        }
        return 0;
    }

    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client
        .plan_from_catalog(&args.id, &args.targets, args.name.as_deref())
        .await
    {
        Ok(plans) => {
            eprintln!(
                "catalog plan(s) created (Pending — approve before deploy); targets: {}",
                args.targets.join(", ")
            );
            emit_change_plans(&plans)
        }
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn help_requires_targets_and_yes() {
        let mut cmd = crate::cli::Cli::command();
        let catalog = cmd.find_subcommand_mut("catalog").expect("catalog");
        let plan = catalog.find_subcommand_mut("plan").expect("plan");
        let mut buf = Vec::new();
        plan.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--targets"));
        assert!(help.contains("--yes"));
        assert!(help.contains("never auto-approved") || help.contains("Pending"));
    }
}
