use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// External plan id.
    #[arg(long)]
    pub plan_id: String,

    /// Rejection reason recorded on the plan (or set `DESLICER_REJECT_REASON`).
    #[arg(long, env = "DESLICER_REJECT_REASON")]
    pub reason: Option<String>,

    /// Environment binding used by the CI proxy to verify the GitHub
    /// Environment reviewer acting on this deployment.
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let reason = match args
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => value.to_string(),
        None => {
            eprintln!("reject requires --reason or DESLICER_REJECT_REASON");
            return 2;
        }
    };

    let (_session, client) =
        match authenticate(&ctx, args.environment.as_deref(), Some(&args.plan_id)).await {
            Ok(pair) => pair,
            Err(err) => return map_cli_error(err),
        };

    let plan = match client.reject(&args.plan_id, &reason).await {
        Ok(plan) => plan,
        Err(err) => return map_cli_error(err),
    };

    emit_change_plan(&plan)
}
