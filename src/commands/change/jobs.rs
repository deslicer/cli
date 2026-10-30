use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::{Client, ExecutionJob, ExecutionSummary};
use crate::Ctx;

#[derive(ClapArgs)]
#[command(after_long_help = super::CHANGE_WORKFLOW_EXAMPLES)]
pub struct Args {
    /// External plan id.
    #[arg(long)]
    pub plan_id: String,

    /// Environment binding for auth / CI proxy resolution.
    #[arg(long)]
    pub environment: Option<String>,

    /// Prefer this execution id instead of the plan's latest execution.
    #[arg(long)]
    pub execution_id: Option<String>,
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
    let summary =
        load_execution_with_jobs(&client, &args.plan_id, args.execution_id.as_deref()).await?;
    print_jobs(&ctx, &args.plan_id, &summary);
    Ok(0)
}

pub(crate) async fn load_execution_with_jobs(
    client: &Client,
    plan_id: &str,
    execution_id: Option<&str>,
) -> Result<ExecutionSummary, CliError> {
    let execution_id = match execution_id {
        Some(id) => id.to_string(),
        None => resolve_latest_execution_id(client, plan_id).await?,
    };
    let summary = client.get_execution(&execution_id).await?;
    if summary.jobs.is_empty() && summary.jobs_total > 0 {
        // Older Observer builds may omit jobs on the summary; still return counts.
        return Ok(summary);
    }
    Ok(summary)
}

async fn resolve_latest_execution_id(client: &Client, plan_id: &str) -> Result<String, CliError> {
    let executions = client.list_plan_executions(plan_id).await?;
    executions
        .into_iter()
        .next()
        .map(|row| row.execution_id)
        .ok_or_else(|| {
            CliError::Other(format!(
                "no executions found for plan {plan_id}; deploy first or pass --execution-id"
            ))
        })
}

fn print_jobs(ctx: &Ctx, plan_id: &str, summary: &ExecutionSummary) {
    match ctx.log_format {
        LogFormat::Json => {
            let _ = crate::json_doc::print_success(&json!({
                "plan_id": plan_id,
                "execution_id": summary.execution_id,
                "status": summary.status,
                "jobs_total": summary.jobs_total,
                "jobs_succeeded": summary.jobs_succeeded,
                "jobs_failed": summary.jobs_failed,
                "jobs": summary.jobs,
            }));
        }
        LogFormat::Human => {
            println!(
                "plan {plan_id} execution {} status {} ({}/{} succeeded, {} failed)",
                summary.execution_id,
                summary.status,
                summary.jobs_succeeded,
                summary.jobs_total,
                summary.jobs_failed
            );
            if summary.jobs.is_empty() {
                println!("(no per-job rows in execution payload)");
                return;
            }
            for job in &summary.jobs {
                println!("{}", format_job_line(job));
            }
        }
    }
}

pub(crate) fn format_job_line(job: &ExecutionJob) -> String {
    let label = job.host.as_deref().or(job.name.as_deref()).unwrap_or("-");
    let mut line = format!(
        "job {}  status {}  retries {}  {}",
        job.id, job.status, job.retry_count, label
    );
    if let Some(attempt) = job.attempt {
        line.push_str(&format!("  attempt {attempt}"));
    }
    if let Some(phase) = job.phase_group_id.as_deref() {
        line.push_str(&format!("  phase {phase}"));
    }
    if let Some(reason) = job.failure_reason.as_deref().filter(|r| !r.is_empty()) {
        line.push_str(&format!("  reason: {reason}"));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_job_line_includes_retry_count() {
        let job = ExecutionJob {
            id: "j1".into(),
            status: "failed".into(),
            retry_count: 2,
            host: Some("idx-1".into()),
            name: None,
            attempt: Some(1),
            failure_reason: Some("disk full".into()),
            phase_group_id: None,
            queued_at: None,
            completed_at: None,
        };
        let line = format_job_line(&job);
        assert!(line.contains("j1"));
        assert!(line.contains("retries 2"));
        assert!(line.contains("idx-1"));
        assert!(line.contains("reason: disk full"));
    }

    #[test]
    fn decodes_observer_job_summary_shape() {
        // Shape of DAP `JobSummary` (observer-api models/executions.rs).
        let raw = r#"{"execution_id":"e1","status":"failed","jobs_total":1,
            "jobs":[{"job_id":"0b4f","host_id":null,"hostname":"idx-1",
            "splunk_version":"9.4.1","status":"failed","attempt":3,"retry_count":2,
            "worker_id":null,"failure_reason":"timeout","role_tier":1,
            "phase_group_id":null,"queued_at":"2026-10-29T10:00:00Z",
            "claimed_at":null,"completed_at":null}]}"#;
        let summary: crate::observer_client::ExecutionSummary =
            serde_json::from_str(raw).expect("JobSummary must decode");
        let job = &summary.jobs[0];
        assert_eq!(job.id, "0b4f");
        assert_eq!(job.host.as_deref(), Some("idx-1"));
        assert_eq!(job.retry_count, 2);
        assert_eq!(job.attempt, Some(3));
    }
}
