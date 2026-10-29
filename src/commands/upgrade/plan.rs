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

pub(crate) fn next_steps(plan_id: &str) -> Vec<String> {
    vec![
        format!("deslicer change approve --plan-id {plan_id}"),
        format!("deslicer change deploy --plan-id {plan_id}"),
    ]
}

async fn run_inner(ctx: &Ctx, args: &RollingArgs, format: LogFormat) -> Result<i32, CliError> {
    super::validate_version(&args.to_version)?;
    let (_session, client) = authenticate(ctx, args.environment.as_deref(), None).await?;
    let body = rolling_request(&client, args).await?;
    let plan = client.create_upgrade_plan(&body).await?;
    Ok(match format {
        LogFormat::Json => print_json(&serde_json::json!({
            "plan_id": plan.plan_id,
            "status": "pending_approval",
            "next_steps": next_steps(&plan.plan_id),
            "plan": plan,
        })),
        LogFormat::Human => {
            print!("{}", format_human(&plan));
            0
        }
    })
}

fn format_human(plan: &UpgradePlanCreated) -> String {
    let steps = next_steps(&plan.plan_id);
    format!(
        "plan_id: {}\nupgrade plan created ({} → {}, {} host(s), {} phase(s)); Pending, nothing was approved or deployed\nnext: {}\n  then: {}\n",
        plan.plan_id,
        plan.family,
        plan.to_version,
        plan.host_ids.len(),
        plan.phase_ids.len(),
        steps[0],
        steps[1]
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
        assert!(text.contains("deslicer change approve --plan-id p-1"));
    }
}
