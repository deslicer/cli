use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::PlanProgress;
use crate::output::emit_plan_progress;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// External plan id.
    #[arg(long)]
    pub plan_id: String,
}

const MAX_ATTEMPTS: u32 = 10;
const INITIAL_DELAY_MS: u64 = 500;

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, None, Some(&args.plan_id)).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(err),
    };

    let mut delay_ms = INITIAL_DELAY_MS;
    let mut last: Option<PlanProgress> = None;

    for attempt in 0..MAX_ATTEMPTS {
        let progress = match client.progress(&args.plan_id).await {
            Ok(progress) => progress,
            Err(err) => return map_cli_error(err),
        };

        if progress.is_terminal() {
            return emit_plan_progress(&progress);
        }

        last = Some(progress);

        if attempt + 1 < MAX_ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            delay_ms = delay_ms.saturating_mul(2);
        }
    }

    match last {
        Some(progress) => emit_plan_progress(&progress),
        None => {
            eprintln!("no progress available for plan {}", args.plan_id);
            1
        }
    }
}
