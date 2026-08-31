use std::io::{IsTerminal, Read};

use clap::Args as ClapArgs;
use uuid::Uuid;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::errors::CliError;
use crate::Ctx;

use super::client::{AgentClient, StartedRun};
use super::ids::parse_conversation_id;
use super::render::{RenderMode, Renderer};
use super::resolve::resolve_agent;
use super::stream::{consume_stream, StreamEnd};

/// Exit code convention for "interrupted by SIGINT" (128 + 2).
const EXIT_INTERRUPTED: i32 = 130;

#[derive(ClapArgs)]
pub struct Args {
    /// Agent name or id. Omit to run the tenant Orchestrator.
    #[arg(long, short = 'a')]
    pub agent: Option<String>,

    /// Prompt text. Omit to read the prompt from stdin.
    pub prompt: Option<String>,

    /// Continue an existing conversation instead of starting a new one
    #[arg(long)]
    pub conversation: Option<String>,

    /// Reuse a key so a retried invocation does not start a second run
    #[arg(long)]
    pub idempotency_key: Option<String>,

    /// Start the run and return immediately, without waiting for the answer
    #[arg(long)]
    pub no_wait: bool,

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
    if let Some(id) = args.conversation.as_deref() {
        parse_conversation_id(id)?;
    }

    let client = AgentClient::from_ctx(&ctx)?;
    // Resolved before the prompt is read, so a mistyped name fails immediately
    // rather than after stdin has been consumed and can no longer be replayed.
    let agent_id = match args.agent.as_deref() {
        Some(raw) => Some(resolve_agent(&client, raw).await?),
        None => None,
    };

    let prompt = resolve_prompt(args.prompt)?;
    // A fresh key per invocation is the safe default: a user who re-runs the
    // command means it. `--idempotency-key` is for CI, where a retried job
    // step should resume rather than duplicate.
    let idempotency_key = args
        .idempotency_key
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let started = client
        .start_run(
            agent_id.as_deref(),
            &prompt,
            args.conversation.as_deref(),
            &idempotency_key,
        )
        .await?;

    let conversation_id = started.conversation_id.clone();

    if args.no_wait {
        return detach(&ctx, started);
    }

    // Held for the interrupt and truncation messages: whichever way the read
    // ends early, the run outlives it and these are the handles that find it.
    let run_id = started.run_id.clone();

    if ctx.log_format == LogFormat::Human {
        if let Some(id) = conversation_id.as_deref() {
            // Printed before the first token so it survives a Ctrl-C, and so a
            // follow-up prompt can be aimed at this thread with --conversation.
            eprintln!("Conversation {id}");
        }
        if args.verbose {
            if let Some(id) = run_id.as_deref() {
                eprintln!("Run {id}");
            }
        }
    }

    let mode = match ctx.log_format {
        LogFormat::Human => RenderMode::Human,
        LogFormat::Json => RenderMode::Json,
    };

    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut renderer = Renderer::new(stdout.lock(), stderr.lock(), mode, args.verbose);

    let end = consume_stream(started.response, &mut renderer, interrupted()).await;
    // Terminate the answer line even when the read failed, so a partial
    // answer is not left glued to the error message.
    let finish = renderer.finish();
    let failure = renderer.failure().map(str::to_string);
    let end = end?;
    finish?;

    if let Some(reason) = failure {
        return Err(CliError::AgentRunFailed(reason));
    }

    match end {
        StreamEnd::Completed => Ok(0),
        StreamEnd::Cancelled => {
            eprintln!(
                "{}",
                cancelled_message(run_id.as_deref(), conversation_id.as_deref())
            );
            Ok(EXIT_INTERRUPTED)
        }
        StreamEnd::Truncated => Err(CliError::AgentRunFailed(truncated_message(
            run_id.as_deref(),
            conversation_id.as_deref(),
        ))),
    }
}

/// Reports the run's handle and hangs up without reading the body.
///
/// The server tees the orchestrator's output before it reaches the wire, so
/// dropping the response here closes this connection without stopping the run.
/// Whatever it produces is still readable through `agent logs`.
fn detach(ctx: &Ctx, started: StartedRun) -> Result<i32, CliError> {
    // A run started with --no-wait can only be found again by its id, so an
    // id the server did not send is a hard failure rather than a warning.
    let run_id = started.run_id.ok_or_else(|| {
        CliError::AgentRunFailed(
            "the run started but the server did not return its id, so it cannot be \
             followed. Check the portal for the result."
                .into(),
        )
    })?;

    match ctx.log_format {
        LogFormat::Json => {
            let line = serde_json::to_string(&serde_json::json!({
                "runId": run_id,
                "conversationId": started.conversation_id,
                "status": "running",
            }))
            .map_err(|e| CliError::Other(format!("encode run handle: {e}")))?;
            println!("{line}");
        }
        LogFormat::Human => {
            // stdout, not stderr: with --no-wait the id is the output.
            println!("{run_id}");
            eprintln!("Started. Follow it with `deslicer agent logs {run_id} --follow`.");
        }
    }

    Ok(0)
}

async fn interrupted() {
    // A failed signal registration must not look like a Ctrl-C, so park
    // forever instead of resolving.
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
}

fn resolve_prompt(arg: Option<String>) -> Result<String, CliError> {
    let prompt = match arg {
        Some(text) => text,
        None => {
            if std::io::stdin().is_terminal() {
                return Err(CliError::Other(
                    "no prompt given. Pass it as an argument or pipe it on stdin:\n  \
                     deslicer agent run \"why did plan X fail?\""
                        .into(),
                ));
            }
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|err| CliError::Other(format!("read prompt from stdin: {err}")))?;
            buf
        }
    };

    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(CliError::Other("the prompt is empty".into()));
    }
    Ok(prompt)
}

/// Names the run so the reader can pick the answer back up.
///
/// The run id is the handle `agent logs` takes; the conversation id is not,
/// and `--conversation` starts a *new* run rather than reattaching to this
/// one. Only fall back to the conversation when the server withheld the run
/// id, and then send the reader to the portal rather than to a command that
/// would start second run.
fn resume_hint(run_id: Option<&str>, conversation_id: Option<&str>) -> Option<String> {
    if let Some(id) = run_id {
        return Some(format!(
            "follow it with `deslicer agent logs {id} --follow`"
        ));
    }
    conversation_id.map(|id| format!("see conversation {id} in the portal"))
}

fn cancelled_message(run_id: Option<&str>, conversation_id: Option<&str>) -> String {
    match resume_hint(run_id, conversation_id) {
        Some(hint) => format!("Interrupted. The run continues server-side; {hint}."),
        None => "Interrupted. The run continues server-side.".to_string(),
    }
}

fn truncated_message(run_id: Option<&str>, conversation_id: Option<&str>) -> String {
    match resume_hint(run_id, conversation_id) {
        Some(hint) => {
            format!("the connection closed before the run finished, but it continues server-side; {hint}.")
        }
        None => "the connection closed before the run finished.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_argument_is_trimmed() {
        let prompt = resolve_prompt(Some("  hello  ".into())).expect("prompt");
        assert_eq!(prompt, "hello");
    }

    #[test]
    fn whitespace_only_prompt_is_rejected() {
        let err = resolve_prompt(Some("   \n".into())).expect_err("should reject");
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn cancelled_message_hands_back_the_command_that_reattaches() {
        let text = cancelled_message(Some("r-1"), Some("c-1"));
        assert!(text.contains("deslicer agent logs r-1 --follow"), "{text}");
    }

    /// `--conversation` starts a second run; suggesting it after a Ctrl-C
    /// would duplicate work and still not show the interrupted answer.
    #[test]
    fn cancelled_message_never_suggests_the_conversation_flag() {
        assert!(!cancelled_message(Some("r-1"), Some("c-1")).contains("--conversation"));
        assert!(!cancelled_message(None, Some("c-1")).contains("--conversation"));
    }

    /// The run id is the only handle `agent logs` accepts, so it wins even
    /// when both are known.
    #[test]
    fn resume_hint_prefers_the_run_over_the_conversation() {
        let hint = resume_hint(Some("r-1"), Some("c-1")).expect("hint");
        assert!(hint.contains("r-1"), "{hint}");
        assert!(!hint.contains("c-1"), "{hint}");
    }

    #[test]
    fn resume_hint_falls_back_to_the_portal_without_a_run_id() {
        let hint = resume_hint(None, Some("c-9")).expect("hint");
        assert!(hint.contains("c-9"), "{hint}");
        assert!(!hint.contains("agent logs"), "{hint}");
    }

    #[test]
    fn resume_hint_is_absent_when_nothing_identifies_the_run() {
        assert!(resume_hint(None, None).is_none());
    }

    #[test]
    fn cancelled_message_without_any_handle_still_reads() {
        assert_eq!(
            cancelled_message(None, None),
            "Interrupted. The run continues server-side."
        );
    }

    #[test]
    fn truncated_message_points_at_the_run() {
        let text = truncated_message(Some("r-9"), Some("c-9"));
        assert!(text.contains("deslicer agent logs r-9"), "{text}");
    }
}
