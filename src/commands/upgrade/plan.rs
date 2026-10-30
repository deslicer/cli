use super::{output_format, print_json, rolling_request, RollingArgs};
use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::UpgradePlanCreated;
use crate::Ctx;

pub async fn run(ctx: Ctx, args: RollingArgs) -> i32 {
    let format = output_format(&ctx, args.json);
    match run_inner(&ctx, &args, format).await {
        Ok(code) => code,
        Err(err) => map_cli_error(format, err),
    }
}

/// Deprecated alias of `next_commands` for `upgrade plan` JSON (kept one release).
pub(crate) fn next_steps(plan_id: &str) -> Vec<String> {
    crate::plan_next::next_commands_for(plan_id, "pending_approval").1
}

async fn run_inner(ctx: &Ctx, args: &RollingArgs, format: LogFormat) -> Result<i32, CliError> {
    super::validate_version(&args.to_version)?;
    let (_session, client) = authenticate(ctx, args.environment.as_deref(), None).await?;
    let body = rolling_request(&client, args).await?;
    let plan = client.create_upgrade_plan(&body).await?;
    let status = "pending_approval";
    let (next_command, next_commands) = crate::plan_next::next_commands_for(&plan.plan_id, status);
    Ok(match format {
        LogFormat::Json => {
            // `mut` matches #254 so `"replayed": true` can insert additively.
            #[allow(unused_mut)]
            let mut doc = serde_json::json!({
                "plan_id": plan.plan_id,
                "status": status,
                "url": serde_json::Value::Null,
                "next_command": next_command,
                "next_commands": next_commands,
                // Deprecated alias of `next_commands` — remove after one release.
                "next_steps": next_steps(&plan.plan_id),
                "plan": plan,
            });
            print_json(&doc)
        }
        LogFormat::Human => {
            print!("{}", format_human(&plan));
            0
        }
    })
}

fn format_human(plan: &UpgradePlanCreated) -> String {
    let steps = next_steps(&plan.plan_id);
    let next = steps.first().cloned().unwrap_or_default();
    let then = steps.get(1).cloned().unwrap_or_default();
    format!(
        "plan_id: {}\nupgrade plan created ({} → {}, {} host(s), {} phase(s)); Pending, nothing was approved or deployed\nnext: {}\n  then: {}\n",
        plan.plan_id,
        plan.family,
        plan.to_version,
        plan.host_ids.len(),
        plan.phase_ids.len(),
        next,
        then
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_prints_plan_id_and_next_step() {
        let plan: UpgradePlanCreated = serde_json::from_value(serde_json::json!({
            "plan_id": "p-1", "family": "upgrade_non_clustered", "catalog_revision": "r",
            "applied_overlay_ids": [], "phase_group_ids": [], "phase_ids": ["a"],
            "host_ids": ["h"], "filename": "splunk.tgz", "sha256": "x",
            "staged_path": "/opt", "to_version": "9.4.1"
        }))
        .unwrap();
        let text = format_human(&plan);
        assert!(text.starts_with("plan_id: p-1\n"));
        assert!(text.contains("deslicer change status --plan-id p-1"));
        assert!(text.contains("deslicer change approve --plan-id p-1"));
    }
}
