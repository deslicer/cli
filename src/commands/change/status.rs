use clap::Args as ClapArgs;
use std::time::{Duration, Instant};

use crate::commands::change::jobs::{format_job_line, load_execution_with_jobs};
use crate::commands::change::lifecycle::{is_terminal_lifecycle, plan_status_exit_code};
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::diff_summary::diff_counts_from_observer_value;
use crate::observer_client::{ChangePlan, Client, PlanProgress};
use crate::output::emit_plan_status;
use crate::Ctx;

const DEFAULT_TIMEOUT_SECS: u64 = 60;
const POLL_INTERVAL_SECS: u64 = 2;

#[derive(ClapArgs)]
#[command(after_long_help = super::CHANGE_WORKFLOW_EXAMPLES)]
pub struct Args {
    /// External plan id returned by `change plan` or `change show`.
    #[arg(long)]
    pub plan_id: String,

    /// Maximum seconds to wait for rollout progress while `progress_status` is `partial`.
    /// Named `--timeout-secs` so it does not collide with the global HTTP `--timeout`.
    #[arg(long = "timeout-secs", default_value_t = DEFAULT_TIMEOUT_SECS)]
    pub timeout_secs: u64,

    /// Also print per-job state (one-shot; skips progress polling).
    #[arg(long)]
    pub jobs: bool,

    /// Prefer this execution id when `--jobs` is set.
    #[arg(long)]
    pub execution_id: Option<String>,
}

async fn load_diff(client: &Client, plan: &ChangePlan) -> Option<crate::diff_summary::DiffCounts> {
    client
        .get_dry_run_diff(&plan.id)
        .await
        .ok()
        .and_then(|body| diff_counts_from_observer_value(&body))
}

async fn progress_best_effort(client: &Client, plan_id: &str) -> PlanProgress {
    match client.progress(plan_id).await {
        Ok(progress) => progress,
        Err(_) => PlanProgress {
            plan_id: plan_id.to_string(),
            progress_status: "not_started".into(),
            total_items: 0,
            fully_completed_items: 0,
        },
    }
}

fn finish(
    plan: Option<&ChangePlan>,
    progress: &PlanProgress,
    diff: Option<&crate::diff_summary::DiffCounts>,
) -> i32 {
    let mut code = emit_plan_status(plan, progress, diff);
    if code == 0 {
        if let Some(plan) = plan {
            code = plan_status_exit_code(&plan.status);
        }
    }
    code
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, None, Some(&args.plan_id)).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    // Always surface plan-not-found (and other load failures) via the error
    // envelope — never print a plain-text line ahead of JSON (issue #260).
    // `--jobs` historically did this (#207); the non-jobs path must match.
    let mut plan = match client.get_plan(&args.plan_id).await {
        Ok(plan) => plan,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    let diff = load_diff(&client, &plan).await;

    // `--jobs` is a one-shot inspector: never enter the partial-progress poll loop.
    if args.jobs {
        let progress = progress_best_effort(&client, &args.plan_id).await;
        let code = finish(Some(&plan), &progress, diff.as_ref());
        match load_execution_with_jobs(&client, &args.plan_id, args.execution_id.as_deref()).await {
            Ok(summary) => {
                if summary.jobs.is_empty() {
                    eprintln!(
                        "(no per-job rows for execution {}; jobs_total={})",
                        summary.execution_id, summary.jobs_total
                    );
                } else {
                    for job in &summary.jobs {
                        println!("{}", format_job_line(job));
                    }
                }
            }
            // Jobs are additive; keep the plan-status exit code when only jobs fail.
            Err(err) => eprintln!("could not load jobs: {err}"),
        }
        return code;
    }

    if is_terminal_lifecycle(&plan.status) {
        let progress = progress_best_effort(&client, &args.plan_id).await;
        return finish(Some(&plan), &progress, diff.as_ref());
    }

    let deadline = Instant::now() + Duration::from_secs(args.timeout_secs);

    loop {
        if let Ok(refreshed) = client.get_plan(&args.plan_id).await {
            if is_terminal_lifecycle(&refreshed.status) {
                let progress = progress_best_effort(&client, &args.plan_id).await;
                return finish(Some(&refreshed), &progress, diff.as_ref());
            }
            plan = refreshed;
        }

        let progress = match client.progress(&args.plan_id).await {
            Ok(progress) => progress,
            Err(err) => return map_cli_error(ctx.log_format, err),
        };

        if progress.is_terminal() || !progress.should_poll_for_updates() {
            return finish(Some(&plan), &progress, diff.as_ref());
        }

        if Instant::now() >= deadline {
            eprintln!(
                "timed out after {}s waiting for rollout progress to finish \
                 (progress_status={}, {}/{} items applied)",
                args.timeout_secs,
                progress.progress_status,
                progress.fully_completed_items,
                progress.total_items
            );
            return finish(Some(&plan), &progress, diff.as_ref());
        }

        eprintln!(
            "waiting for rollout progress ({}/{} items applied, progress_status={})...",
            progress.fully_completed_items, progress.total_items, progress.progress_status
        );
        tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
    }
}
