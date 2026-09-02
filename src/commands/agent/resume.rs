//! Continues this session's last conversation.

use std::io::IsTerminal;

use clap::Args as ClapArgs;

use crate::commands::pipeline::map_cli_error;
use crate::errors::CliError;
use crate::Ctx;

use super::client::AgentClient;
use super::logs;
use super::repl;
use super::resolve::resolve_agent;
use super::run::resolve_prompt;
use super::session::{cancelled_message, start_and_stream, truncated_message};
use super::stream::StreamEnd;
use super::types::RunListItem;

#[derive(ClapArgs)]
pub struct Args {
    /// Follow-up prompt. Omit on a TTY to enter the REPL.
    pub prompt: Option<String>,

    /// Agent name or id. Omit to keep the last run's agent.
    #[arg(long, short = 'a')]
    pub agent: Option<String>,

    /// Attach to the last run if it is still going, instead of starting a new one
    #[arg(long, short = 'f')]
    pub follow: bool,

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
    let client = AgentClient::from_ctx(&ctx)?;
    let latest = client.try_latest_run().await?.ok_or_else(no_runs_yet)?;

    if args.follow {
        return Ok(logs::run(
            ctx,
            logs::Args {
                run_id: Some(latest.run_id),
                follow: true,
                verbose: args.verbose,
            },
        )
        .await);
    }

    let conversation_id = latest
        .conversation_id
        .clone()
        .ok_or_else(|| no_conversation(&latest))?;

    if args.prompt.is_none() && std::io::stdin().is_terminal() {
        let detached = (latest.status == "running").then_some(latest.run_id.clone());
        return Ok(repl::run_with(
            ctx,
            repl::Args {
                agent: args.agent.or(latest.agent_id),
                conversation: Some(conversation_id),
                verbose: args.verbose,
            },
            detached,
        )
        .await);
    }

    if args.prompt.is_none() && !std::io::stdin().is_terminal() {
        return Err(CliError::Other(
            "no prompt given. Pass one, or run `deslicer agent resume` on a terminal.".into(),
        ));
    }

    if latest.status == "running" {
        return Err(CliError::Other(format!(
            "run {} is still going. Follow it with `deslicer agent logs {} --follow`, \
             or pass --follow.",
            latest.run_id, latest.run_id
        )));
    }

    let agent_id = match args.agent.as_deref() {
        Some(raw) => Some(resolve_agent(&client, raw).await?),
        None => latest.agent_id.clone(),
    };
    let prompt = resolve_prompt(args.prompt)?;

    let turn = start_and_stream(
        &ctx,
        &client,
        agent_id.as_deref(),
        &prompt,
        Some(&conversation_id),
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

fn no_runs_yet() -> CliError {
    CliError::Other("no runs yet. Start one with `deslicer agent` or `deslicer agent run`.".into())
}

fn no_conversation(latest: &RunListItem) -> CliError {
    CliError::Other(format!(
        "the last run ({}) has no conversation to continue. Start a new one with \
         `deslicer agent`.",
        latest.run_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_runs_points_at_the_repl() {
        let text = no_runs_yet().to_string();
        assert!(text.contains("deslicer agent"), "{text}");
    }

    #[test]
    fn a_run_without_a_conversation_does_not_invent_one() {
        let latest = RunListItem {
            run_id: "r-1".into(),
            status: "succeeded".into(),
            agent_id: None,
            conversation_id: None,
            started_at: "2026-08-31T12:00:00.000Z".into(),
            finished_at: None,
            prompt_preview: None,
        };
        let text = no_conversation(&latest).to_string();
        assert!(text.contains("r-1"), "{text}");
        assert!(text.contains("deslicer agent"), "{text}");
    }
}
