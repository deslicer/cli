use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::ReconcileMode;
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,

    #[arg(long, default_value = "plan-only")]
    pub mode: String,
}

fn parse_mode(mode: &str) -> Result<ReconcileMode, ()> {
    match mode {
        "plan-only" => Ok(ReconcileMode::PlanOnly),
        "apply" => Ok(ReconcileMode::Apply),
        _ => Err(()),
    }
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let mode = match parse_mode(&args.mode) {
        Ok(m) => m,
        Err(()) => {
            eprintln!("invalid mode: expected \"plan-only\" or \"apply\"");
            return 1;
        }
    };

    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(err),
    };

    let plan = match client.reconcile(&args.environment, mode).await {
        Ok(plan) => plan,
        Err(err) => return map_cli_error(err),
    };

    emit_change_plan(&plan)
}
