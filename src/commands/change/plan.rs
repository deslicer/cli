use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error, require_proxy_mode};
use crate::observer_client::{ChangePlan, Client, OrchestratedPlan};
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,

    /// Return immediately after the compile-runner is triggered instead of
    /// waiting for the plan to reach pending_approval.
    #[arg(long)]
    pub no_wait: bool,
}

/// Compile polling: the ephemeral compile-runner takes seconds to a few
/// minutes to clone, parse, diff, and post the plan draft.
const COMPILE_POLL_ATTEMPTS: u32 = 60;
const COMPILE_POLL_INTERVAL_SECS: u64 = 5;

fn is_still_compiling(status: &str) -> bool {
    matches!(status, "draft" | "compiling" | "compile_pending")
}

fn is_compile_failure(status: &str) -> bool {
    matches!(status, "failed" | "compile_failed" | "rejected")
}

async fn wait_for_compile(client: &Client, plan_id: &str) -> Result<ChangePlan, String> {
    let mut last_err: Option<String> = None;
    for _ in 0..COMPILE_POLL_ATTEMPTS {
        match client.get_plan(plan_id).await {
            Ok(plan) if !is_still_compiling(&plan.status) => return Ok(plan),
            Ok(_) => last_err = None,
            // The draft row may not be visible yet right after creation.
            Err(err) => last_err = Some(err.to_string()),
        }
        tokio::time::sleep(std::time::Duration::from_secs(COMPILE_POLL_INTERVAL_SECS)).await;
    }
    Err(last_err.unwrap_or_else(|| {
        format!(
            "plan {plan_id} did not finish compiling within {}s",
            u64::from(COMPILE_POLL_ATTEMPTS) * COMPILE_POLL_INTERVAL_SECS
        )
    }))
}

fn orchestrated_as_change_plan(created: &OrchestratedPlan) -> ChangePlan {
    ChangePlan {
        id: created.plan_row_id.clone().unwrap_or_default(),
        plan_id: Some(created.plan_id.clone()),
        status: created.status.clone(),
        name: None,
        summary: None,
    }
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(err),
    };

    if let Err(err) = require_proxy_mode(&session, "change plan") {
        return map_cli_error(err);
    }

    let created = match client
        .create_plan_orchestrated(args.environment.as_deref())
        .await
    {
        Ok(created) => created,
        Err(err) => return map_cli_error(err),
    };

    // Older proxy builds return only the internal row id, which cannot be
    // polled through GET /plans/{plan_id} — skip waiting in that case.
    if args.no_wait || created.plan_row_id.is_none() {
        return emit_change_plan(&orchestrated_as_change_plan(&created));
    }

    let plan = match wait_for_compile(&client, &created.plan_id).await {
        Ok(plan) => plan,
        Err(msg) => {
            eprintln!("plan compile did not complete: {msg}");
            emit_change_plan(&orchestrated_as_change_plan(&created));
            return 1;
        }
    };

    if is_compile_failure(&plan.status) {
        eprintln!("plan compile failed with status: {}", plan.status);
        emit_change_plan(&plan);
        return 1;
    }

    emit_change_plan(&plan)
}
