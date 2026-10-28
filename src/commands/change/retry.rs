use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::change::jobs::load_execution_with_jobs;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::output::emit_execution_queued;
use crate::Ctx;

#[derive(ClapArgs)]
#[command(after_long_help = super::CHANGE_WORKFLOW_EXAMPLES)]
pub struct Args {
    /// External plan id.
    #[arg(long)]
    pub plan_id: String,

    /// Retry only this job id (requires resolving the plan's latest execution).
    #[arg(long)]
    pub job: Option<String>,

    /// Prefer this execution id when retrying a single `--job`.
    #[arg(long)]
    pub execution_id: Option<String>,

    /// Environment binding for auth / CI proxy resolution.
    #[arg(long)]
    pub environment: Option<String>,

    /// Perform the retry. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let log_format = ctx.log_format;
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(log_format, err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    let (_session, client) =
        authenticate(&ctx, args.environment.as_deref(), Some(&args.plan_id)).await?;

    if !args.yes {
        print_dry_run(&ctx, &args);
        return Ok(0);
    }

    if let Some(job_id) = args.job.as_deref() {
        let summary =
            load_execution_with_jobs(&client, &args.plan_id, args.execution_id.as_deref()).await?;
        let job = client
            .retry_execution_job(&summary.execution_id, job_id)
            .await?;
        match ctx.log_format {
            LogFormat::Json => println!(
                "{}",
                json!({
                    "retried": true,
                    "plan_id": args.plan_id,
                    "execution_id": summary.execution_id,
                    "job": job,
                })
            ),
            LogFormat::Human => {
                println!(
                    "retried job {} on execution {} (status {}, retries {})",
                    job.id, summary.execution_id, job.status, job.retry_count
                );
            }
        }
        return Ok(0);
    }

    let queued = client.retry_plan(&args.plan_id).await?;
    match ctx.log_format {
        LogFormat::Json => {
            let _ = emit_execution_queued(&queued);
        }
        LogFormat::Human => {
            println!(
                "retry queued as execution {} ({} jobs)",
                queued.execution_id, queued.jobs_total
            );
        }
    }
    Ok(0)
}

fn print_dry_run(ctx: &Ctx, args: &Args) {
    let scope = match args.job.as_deref() {
        Some(job) => format!("job {job}"),
        None => "all failed jobs on the plan".into(),
    };
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            json!({
                "dry_run": true,
                "plan_id": args.plan_id,
                "job": args.job,
                "execution_id": args.execution_id,
                "action": "retry",
            })
        ),
        LogFormat::Human => println!(
            "dry-run: would retry {scope} for plan {}\npass --yes to perform the retry",
            args.plan_id
        ),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn help_requires_yes_for_mutation() {
        let mut cmd = crate::cli::Cli::command();
        let change = cmd.find_subcommand_mut("change").expect("change");
        let retry = change.find_subcommand_mut("retry").expect("retry");
        let mut buf = Vec::new();
        retry.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--yes"));
        assert!(help.contains("--job"));
        assert!(help.contains("--plan-id"));
    }
}
