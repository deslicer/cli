use std::io::{IsTerminal, Read};

use clap::Args as ClapArgs;
use uuid::Uuid;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::errors::CliError;
use crate::Ctx;

use super::client::{AgentClient, StartedRun};
use super::ids::parse_conversation_id;
use super::resolve::resolve_agent;
use super::session::{cancelled_message, start_and_stream, truncated_message};
use super::stream::StreamEnd;

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
    let log_format = ctx.log_format;
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(log_format, err),
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

    if args.no_wait {
        return detach_now(
            &ctx,
            &client,
            agent_id.as_deref(),
            &prompt,
            args.conversation.as_deref(),
            args.idempotency_key.as_deref(),
        )
        .await;
    }

    let turn = start_and_stream(
        &ctx,
        &client,
        agent_id.as_deref(),
        &prompt,
        args.conversation.as_deref(),
        args.verbose,
    )
    .await?;

    if let Some(reason) = turn.failure {
        return Err(CliError::AgentRunFailed(reason));
    }

    match turn.end {
        StreamEnd::Completed => Ok(0),
        StreamEnd::Cancelled => {
            eprintln!(
                "{}",
                cancelled_message(turn.run_id.as_deref(), turn.conversation_id.as_deref())
            );
            Ok(super::session::EXIT_INTERRUPTED)
        }
        StreamEnd::Truncated => Err(CliError::AgentRunFailed(truncated_message(
            turn.run_id.as_deref(),
            turn.conversation_id.as_deref(),
        ))),
    }
}

async fn detach_now(
    ctx: &Ctx,
    client: &AgentClient,
    agent_id: Option<&str>,
    prompt: &str,
    conversation_id: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<i32, CliError> {
    let idempotency_key = idempotency_key
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let started = client
        .start_run(agent_id, prompt, conversation_id, &idempotency_key)
        .await?;
    detach(ctx, started)
}

/// Reports the run's handle and hangs up without reading the body.
///
/// The server tees the orchestrator's output before it reaches the wire, so
/// dropping the response here closes this connection without stopping the run.
/// Whatever it produces is still readable through `agent logs`.
fn detach(ctx: &Ctx, started: StartedRun) -> Result<i32, CliError> {
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
            println!("{run_id}");
            eprintln!("Started. Follow it with `deslicer agent logs {run_id} --follow`.");
        }
    }

    Ok(0)
}

pub fn resolve_prompt(arg: Option<String>) -> Result<String, CliError> {
    let prompt = match arg {
        Some(text) => text,
        None => {
            if std::io::stdin().is_terminal() {
                return Err(CliError::Other(
                    "no prompt given. Pass it as an argument, or start a conversation \
                     with `deslicer agent`:\n  deslicer agent run \"why did plan X fail?\""
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
}
