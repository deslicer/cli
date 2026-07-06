use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error, require_proxy_mode};
use crate::output::{emit_change_plan, emit_message};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// External plan id (the `plan_id` output of `change plan`).
    #[arg(long)]
    pub plan_id: String,

    #[arg(long)]
    pub environment: Option<String>,

    /// Git ref for the dry-run compile; defaults to the commit the plan was
    /// created from.
    #[arg(long)]
    pub git_ref: Option<String>,
}

/// Extract a change summary from the persisted dry-run diff JSON
/// (compile_runner_dry_run_v1 shape: summary.{additions,modifications,deletions,total}).
fn diff_summary_pairs(diff: &serde_json::Value) -> Vec<(&'static str, String)> {
    let summary = diff.get("diff_json").unwrap_or(diff).get("summary");
    let count = |key: &str| -> String {
        summary
            .and_then(|s| s.get(key))
            .and_then(|v| v.as_u64())
            .map(|n| n.to_string())
            .unwrap_or_default()
    };
    vec![
        ("diff_total", count("total")),
        ("diff_additions", count("additions")),
        ("diff_modifications", count("modifications")),
        ("diff_deletions", count("deletions")),
    ]
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (session, client) =
        match authenticate(&ctx, args.environment.as_deref(), Some(&args.plan_id)).await {
            Ok(pair) => pair,
            Err(err) => return map_cli_error(err),
        };

    if let Err(err) = require_proxy_mode(&session, "change verify") {
        return map_cli_error(err);
    }

    // Resolve the internal row id — the compile-runner and diff endpoints
    // are keyed by it, while --plan-id carries the external identifier.
    let plan = match client.get_plan(&args.plan_id).await {
        Ok(plan) => plan,
        Err(err) => return map_cli_error(err),
    };

    if let Err(err) = client
        .verify_plan_orchestrated(&plan.id, args.git_ref.as_deref())
        .await
    {
        eprintln!("verification failed: {err}");
        return map_cli_error(err);
    }

    // The diff is best-effort output: verification already succeeded above.
    match client.get_dry_run_diff(&plan.id).await {
        Ok(diff) => {
            println!("{}", serde_json::to_string(&diff).unwrap_or_default());
            emit_message(&diff_summary_pairs(&diff));
        }
        Err(err) => {
            eprintln!("dry-run accepted, but the diff could not be fetched: {err}");
        }
    }

    emit_change_plan(&plan)
}
