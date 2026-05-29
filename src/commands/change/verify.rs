use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::ReconcileMode;
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub plan_id: String,

    #[arg(long)]
    pub environment: Option<String>,
}

fn verification_failed(plan: &crate::observer_client::ChangePlan) -> bool {
    if plan.status == "failed" {
        return true;
    }
    plan.summary
        .as_ref()
        .is_some_and(|summary| !summary.is_empty())
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let _ = &args.plan_id;
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(err),
    };

    let plan = match client
        .reconcile(&args.environment, ReconcileMode::PlanOnly)
        .await
    {
        Ok(plan) => plan,
        Err(err) => return map_cli_error(err),
    };

    if verification_failed(&plan) {
        eprintln!("verification failed: drift detected");
        return 1;
    }

    emit_change_plan(&plan)
}
