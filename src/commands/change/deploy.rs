use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::{Client, ExecutionSummary};
use crate::output::{emit_execution_queued, emit_execution_summary};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// External plan id of an approved plan.
    #[arg(long)]
    pub plan_id: String,

    #[arg(long)]
    pub environment: Option<String>,

    /// Return immediately after queueing instead of monitoring the rollout.
    #[arg(long)]
    pub no_wait: bool,
}

/// Rollout monitoring: worker leases + rolling waves can take many minutes.
const EXECUTION_POLL_ATTEMPTS: u32 = 120;
const EXECUTION_POLL_INTERVAL_SECS: u64 = 10;

async fn wait_for_execution(
    client: &Client,
    execution_id: &str,
) -> Result<ExecutionSummary, String> {
    let mut last: Option<ExecutionSummary> = None;
    for _ in 0..EXECUTION_POLL_ATTEMPTS {
        match client.get_execution(execution_id).await {
            Ok(summary) if summary.is_terminal() => return Ok(summary),
            Ok(summary) => last = Some(summary),
            Err(err) => return Err(err.to_string()),
        }
        tokio::time::sleep(std::time::Duration::from_secs(EXECUTION_POLL_INTERVAL_SECS)).await;
    }
    let last_status = last.map(|s| s.status).unwrap_or_default();
    Err(format!(
        "execution {execution_id} still `{last_status}` after {}s",
        u64::from(EXECUTION_POLL_ATTEMPTS) * EXECUTION_POLL_INTERVAL_SECS
    ))
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) =
        match authenticate(&ctx, args.environment.as_deref(), Some(&args.plan_id)).await {
            Ok(pair) => pair,
            Err(err) => return map_cli_error(err),
        };

    let queued = match client.execute(&args.plan_id).await {
        Ok(queued) => queued,
        Err(err) => return map_cli_error(err),
    };

    if args.no_wait {
        return emit_execution_queued(&queued);
    }

    println!(
        "execution {} queued ({} jobs); monitoring rollout...",
        queued.execution_id, queued.jobs_total
    );

    let summary = match wait_for_execution(&client, &queued.execution_id).await {
        Ok(summary) => summary,
        Err(msg) => {
            eprintln!("deploy monitoring failed: {msg}");
            emit_execution_queued(&queued);
            return 1;
        }
    };

    let exit = if summary.is_success() {
        0
    } else {
        eprintln!(
            "execution finished with status `{}` ({} failed, {} partial, {} timed out)",
            summary.status, summary.jobs_failed, summary.jobs_partial, summary.jobs_timed_out
        );
        1
    };

    let emit_code = emit_execution_summary(&summary);
    if exit != 0 {
        exit
    } else {
        emit_code
    }
}
