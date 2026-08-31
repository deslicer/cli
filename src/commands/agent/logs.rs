//! Reattaches to a run that is already in flight, or reads back a finished one.
//!
//! Two transports, because the server has two. A live run can usually be
//! streamed, which is what `--follow` wants. When there is no stream to attach
//! to — no Redis in the deployment, or the buffer already consumed — the same
//! output is available as a single persisted read, so this falls back to
//! polling rather than reporting that the run is unreachable.

use std::io::Write;
use std::time::Duration;

use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::errors::CliError;
use crate::Ctx;

use super::client::{AgentClient, RunStatus};
use super::ids::parse_run_id;
use super::render::{RenderMode, Renderer};
use super::stream::{consume_stream, StreamEnd};

/// Exit code convention for "interrupted by SIGINT" (128 + 2).
const EXIT_INTERRUPTED: i32 = 130;

/// Gap between polls when there is no stream to attach to.
///
/// Short enough that a run finishing feels immediate, long enough that a
/// `--follow` left open for the full ten-minute ceiling costs a few hundred
/// requests rather than tens of thousands.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(ClapArgs)]
pub struct Args {
    /// Run id, printed when the run started. Omit to follow the latest run.
    pub run_id: Option<String>,

    /// Wait for the run to finish instead of reporting where it got to
    #[arg(long, short = 'f')]
    pub follow: bool,

    /// Show the agent's reasoning as it streams
    #[arg(long)]
    pub verbose: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    if let Some(id) = args.run_id.as_deref() {
        // Before the session is resolved, so a mistyped id on a machine that
        // has never logged in reports the typo rather than the missing session.
        parse_run_id(id)?;
    }

    let client = AgentClient::from_ctx(&ctx)?;
    let run_id = match args.run_id.as_deref() {
        Some(id) => id.to_string(),
        None => client.latest_run().await?.run_id,
    };

    // Read the status first so a bad id, an expired session, or someone
    // else's run fails on a cheap request rather than after opening a stream.
    let status = client.run_status(&run_id).await?;

    if !args.follow || status.is_terminal() {
        // Nothing to wait for; --follow degrades to a plain read.
        return report_once(&ctx, &client, &run_id).await;
    }

    if ctx.log_format == LogFormat::Human {
        // The poll fallback can sit silent for a couple of seconds and the
        // stream can take about as long to produce its first token; without
        // this the terminal looks hung.
        eprintln!("Following run {run_id}. Ctrl-C detaches.");
    }

    if let Some(response) = client.resume_run(&run_id).await? {
        let end = stream_into_terminal(&ctx, &args, response).await?;
        return match end {
            StreamEnd::Cancelled => {
                eprintln!("Detached. The run continues server-side.");
                Ok(EXIT_INTERRUPTED)
            }
            // The stream ending does not itself say how the run ended — a
            // truncated body looks the same from here — so the ledger has
            // the last word.
            StreamEnd::Completed | StreamEnd::Truncated => {
                let status = client.run_status(&run_id).await?;
                exit_for(status)
            }
        };
    }

    poll_until_terminal(&client, &run_id).await?;
    report_once(&ctx, &client, &run_id).await
}

/// Prints whatever the run has produced and exits on its outcome.
///
/// Re-reads rather than taking the status the caller already holds: the output
/// endpoint returns a strictly fresher view of the same row, and carries the
/// answer with it.
async fn report_once(ctx: &Ctx, client: &AgentClient, run_id: &str) -> Result<i32, CliError> {
    let output = client.run_output(run_id).await?;

    match ctx.log_format {
        LogFormat::Json => {
            let line = serde_json::to_string(&serde_json::json!({
                "runId": output.status.run_id,
                "status": output.status.status,
                "conversationId": output.status.conversation_id,
                "errorCode": output.status.error_code,
                "output": output.output,
            }))
            .map_err(|e| CliError::Other(format!("encode run output: {e}")))?;
            println!("{line}");
        }
        LogFormat::Human => {
            if let Some(text) = output.output.as_deref() {
                // Straight to stdout, unadorned: this is the thing a script
                // pipes onward.
                println!("{text}");
                std::io::stdout()
                    .flush()
                    .map_err(|e| CliError::Other(format!("write run output: {e}")))?;
            } else if !output.status.is_terminal() {
                eprintln!("Run is still going. Re-run with --follow to wait for it.");
            } else {
                eprintln!("The run produced no output.");
            }
        }
    }

    exit_for(output.status)
}

async fn stream_into_terminal(
    ctx: &Ctx,
    args: &Args,
    response: reqwest::Response,
) -> Result<StreamEnd, CliError> {
    let mode = match ctx.log_format {
        LogFormat::Human => RenderMode::Human,
        LogFormat::Json => RenderMode::Json,
    };

    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut renderer = Renderer::new(stdout.lock(), stderr.lock(), mode, args.verbose);

    let end = consume_stream(response, &mut renderer, interrupted()).await;
    // Close the answer line even on a read failure, so a partial answer is
    // not left glued to the error message.
    let finish = renderer.finish();
    let end = end?;
    finish?;
    Ok(end)
}

/// Waits for the ledger to settle, since there is no stream to watch.
///
/// No deadline of its own: the server marks a run that outlived the platform
/// ceiling as failed, so this always terminates on a status change rather
/// than on a timer the CLI would have to keep in step with the server's.
async fn poll_until_terminal(client: &AgentClient, run_id: &str) -> Result<RunStatus, CliError> {
    loop {
        tokio::select! {
            biased;
            () = interrupted() => {
                return Err(CliError::Other(
                    "Detached. The run continues server-side.".into(),
                ));
            }
            () = tokio::time::sleep(POLL_INTERVAL) => {}
        }

        let status = client.run_status(run_id).await?;
        if status.is_terminal() {
            return Ok(status);
        }
    }
}

fn exit_for(status: RunStatus) -> Result<i32, CliError> {
    match status.status.as_str() {
        "succeeded" => Ok(0),
        "failed" => Err(CliError::AgentRunFailed(failure_message(&status))),
        // Only reachable without --follow, where reporting the state *is* the
        // job. Zero, because nothing went wrong.
        _ => Ok(0),
    }
}

fn failure_message(status: &RunStatus) -> String {
    match status.error_code.as_deref() {
        Some("abandoned") => {
            "the run was cut short by the server before it finished. Start it again.".to_string()
        }
        Some(code) => format!("the run failed ({code})"),
        None => "the run failed".to_string(),
    }
}

async fn interrupted() {
    // A failed signal registration must not look like a Ctrl-C, so park
    // forever instead of resolving.
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: &str, error_code: Option<&str>) -> RunStatus {
        RunStatus {
            run_id: "55555555-5555-4555-8555-555555555555".into(),
            status: state.into(),
            conversation_id: None,
            error_code: error_code.map(str::to_string),
        }
    }

    #[test]
    fn a_succeeded_run_exits_zero() {
        assert_eq!(exit_for(status("succeeded", None)).expect("ok"), 0);
    }

    #[test]
    fn a_failed_run_exits_on_the_agent_failure_code() {
        let err = exit_for(status("failed", Some("run_failed"))).expect_err("should fail");
        assert_eq!(err.exit_code(), 13);
    }

    #[test]
    fn reading_a_still_running_run_is_not_an_error() {
        // Without --follow, reporting the state is the whole job.
        assert_eq!(exit_for(status("running", None)).expect("ok"), 0);
    }

    #[test]
    fn an_abandoned_run_says_to_start_it_again() {
        // "abandoned" is the server's word for a run the platform killed
        // mid-flight; retrying is the only thing the caller can do.
        let text = failure_message(&status("failed", Some("abandoned")));
        assert!(text.contains("again"), "{text}");
    }

    #[test]
    fn an_unknown_failure_code_still_reaches_the_user() {
        let text = failure_message(&status("failed", Some("billing_blocked")));
        assert!(text.contains("billing_blocked"), "{text}");
    }
}
