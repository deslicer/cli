use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::diff_summary::diff_counts_from_observer_value;
use crate::output::emit_plan_status;
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

    let plan = match client.get_plan(&args.plan_id).await {
        Ok(plan) => Some(plan),
        Err(err) => {
            eprintln!("could not load plan lifecycle status: {err}");
            None
        }
    };

    let diff = if let Some(ref p) = plan {
        client
            .get_dry_run_diff(&p.id)
            .await
            .ok()
            .and_then(|body| diff_counts_from_observer_value(&body))
    } else {
        None
    };

    let mut delay_ms = INITIAL_DELAY_MS;
    let mut last = None;

    for attempt in 0..MAX_ATTEMPTS {
        let progress = match client.progress(&args.plan_id).await {
            Ok(progress) => progress,
            Err(err) => return map_cli_error(err),
        };

        if progress.is_terminal() {
            return emit_plan_status(plan.as_ref(), &progress, diff.as_ref());
        }

        last = Some(progress);

        if attempt + 1 < MAX_ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            delay_ms = delay_ms.saturating_mul(2);
        }
    }

    match last {
        Some(progress) => emit_plan_status(plan.as_ref(), &progress, diff.as_ref()),
        None => {
            eprintln!("no progress available for plan {}", args.plan_id);
            1
        }
    }
}
