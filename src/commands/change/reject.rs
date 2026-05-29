use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub plan_id: String,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, None, None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(err),
    };

    let plan = match client.reject(&args.plan_id).await {
        Ok(plan) => plan,
        Err(err) => return map_cli_error(err),
    };

    emit_change_plan(&plan)
}
