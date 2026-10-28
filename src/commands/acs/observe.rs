use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// ACS stack name (from `acs stacks list`)
    pub stack: String,

    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    let report = match client.observe_acs_stack(&args.stack).await {
        Ok(report) => report,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&report) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize ACS observe report: {err}");
                1
            }
        },
        LogFormat::Human => {
            println!("ACS observe (read-only) for stack {}", report.stack);
            if let Some(summary) = report.summary.as_deref() {
                println!("summary: {summary}");
            }
            if let Some(plan) = report.project_plan.as_ref() {
                match serde_json::to_string_pretty(plan) {
                    Ok(json) => println!("{json}"),
                    Err(err) => {
                        eprintln!("failed to format project plan: {err}");
                        return 1;
                    }
                }
            }
            eprintln!(
                "Observe never applies changes. Create a Pending plan with:\n  \
                 deslicer change plan --destination acs-stack:{}",
                args.stack
            );
            0
        }
    }
}
