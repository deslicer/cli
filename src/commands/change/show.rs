use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub plan_id: Option<String>,

    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(err),
    };

    if let Some(plan_id) = args.plan_id {
        let plan = match client.get_plan(&plan_id).await {
            Ok(plan) => plan,
            Err(err) => return map_cli_error(err),
        };
        return emit_change_plan(&plan);
    }

    let plans = match client.list_plans(args.environment.as_deref()).await {
        Ok(plans) => plans,
        Err(err) => return map_cli_error(err),
    };

    match serde_json::to_string(&plans) {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(err) => {
            eprintln!("failed to serialize plans: {err}");
            1
        }
    }
}
