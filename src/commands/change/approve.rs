use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// External plan id.
    #[arg(long)]
    pub plan_id: String,

    /// Environment binding used by the CI proxy to verify the GitHub
    /// Environment reviewer who approved this deployment.
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) =
        match authenticate(&ctx, args.environment.as_deref(), Some(&args.plan_id)).await {
            Ok(pair) => pair,
            Err(err) => return map_cli_error(err),
        };

    let plan = match client.approve(&args.plan_id).await {
        Ok(plan) => plan,
        Err(err) => return map_cli_error(err),
    };

    emit_change_plan(&plan)
}
