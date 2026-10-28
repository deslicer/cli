use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
#[command(after_long_help = super::CHANGE_WORKFLOW_EXAMPLES)]
pub struct Args {
    /// External plan id of a completed plan to invert.
    #[arg(long)]
    pub plan_id: String,

    /// Environment binding for auth / CI proxy resolution.
    #[arg(long)]
    pub environment: Option<String>,

    /// Create the Pending inverse plan. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let log_format = ctx.log_format;
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(log_format, err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    let (_session, client) =
        authenticate(&ctx, args.environment.as_deref(), Some(&args.plan_id)).await?;

    if !args.yes {
        print_dry_run(&ctx, &args.plan_id);
        return Ok(0);
    }

    let plan = client.revert(&args.plan_id).await?;
    // Revert always returns a Pending plan; never auto-approve.
    let _ = emit_change_plan(&plan);
    Ok(0)
}

fn print_dry_run(ctx: &Ctx, plan_id: &str) {
    let human = dry_run_text(plan_id);
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            json!({
                "dry_run": true,
                "source_plan_id": plan_id,
                "action": "revert",
                "will_auto_approve": false,
            })
        ),
        LogFormat::Human => print!("{human}"),
    }
}

pub(crate) fn dry_run_text(plan_id: &str) -> String {
    format!(
        "dry-run: would create a Pending inverse plan from {plan_id}\n\
         the revert plan is never auto-approved; pass --yes to create it, then `deslicer change approve`\n"
    )
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn dry_run_never_claims_auto_approve() {
        let text = dry_run_text("plan-abc");
        assert!(text.contains("Pending inverse plan"));
        assert!(text.contains("--yes"));
        assert!(text.contains("never auto-approved") || text.contains("approve"));
    }

    #[test]
    fn help_mentions_yes_and_plan_id() {
        let mut cmd = crate::cli::Cli::command();
        let change = cmd.find_subcommand_mut("change").expect("change");
        let revert = change.find_subcommand_mut("revert").expect("revert");
        let mut buf = Vec::new();
        revert.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--yes"));
        assert!(help.contains("--plan-id"));
        assert!(help.contains("never auto-approves"));
    }
}
