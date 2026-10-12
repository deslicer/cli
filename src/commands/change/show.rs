use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::output::{emit_change_plan, emit_change_plan_ci_outputs};
use crate::Ctx;

#[derive(ClapArgs)]
#[command(after_long_help = super::CHANGE_WORKFLOW_EXAMPLES)]
pub struct Args {
    /// External plan id (omit to list plans for `--environment`).
    #[arg(long)]
    pub plan_id: Option<String>,

    /// Environment name or tenant slug used to scope plan listing and CI auth.
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    if let Some(plan_id) = args.plan_id {
        let plan = match client.get_plan(&plan_id).await {
            Ok(plan) => plan,
            Err(err) => return map_cli_error(ctx.log_format, err),
        };
        return match ctx.log_format {
            LogFormat::Json => emit_change_plan(&plan),
            LogFormat::Human => {
                println!("Plan ID: {}", human_field(Some(plan.external_id())));
                println!("Status: {}", human_field(Some(&plan.status)));
                println!("Name: {}", human_field(plan.name.as_deref()));
                println!("Summary: {}", human_field(plan.summary.as_deref()));
                emit_change_plan_ci_outputs(&plan)
            }
        };
    }

    let plans = match client.list_plans(args.environment.as_deref()).await {
        Ok(plans) => plans,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    if matches!(ctx.log_format, LogFormat::Human) {
        if plans.is_empty() {
            println!("No change plans found.");
        } else {
            println!("PLAN ID  STATUS  NAME");
            for plan in &plans {
                println!(
                    "{}  {}  {}",
                    human_field(Some(plan.external_id())),
                    human_field(Some(&plan.status)),
                    human_field(plan.name.as_deref()),
                );
            }
        }
        return 0;
    }

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

/// Keep server-provided text on one line and prevent terminal control sequences.
fn human_field(value: Option<&str>) -> String {
    let clean: String = value
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() {
        "-".into()
    } else {
        clean
    }
}
