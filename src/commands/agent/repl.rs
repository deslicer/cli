//! Line-oriented conversation loop. Not a TUI.

use std::io::{self, IsTerminal, Write};

use clap::Args as ClapArgs;

use crate::commands::pipeline::map_cli_error;
use crate::errors::CliError;
use crate::Ctx;

use super::client::AgentClient;
use super::ids::parse_conversation_id;
use super::resolve::resolve_agent;
use super::session::{cancelled_message, start_and_stream};
use super::stream::StreamEnd;

#[derive(ClapArgs)]
pub struct Args {
    /// Agent name or id. Omit to run the tenant Orchestrator.
    #[arg(long, short = 'a')]
    pub agent: Option<String>,

    /// Continue this conversation instead of starting a new one
    #[arg(long)]
    pub conversation: Option<String>,

    /// Show the agent's reasoning as it streams
    #[arg(long)]
    pub verbose: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum ReplLine {
    Empty,
    Exit,
    Help,
    Prompt(String),
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    run_with(ctx, args, None).await
}

/// Same loop as `run`, with a run that is already in flight on this thread.
pub async fn run_with(ctx: Ctx, args: Args, detached_run: Option<String>) -> i32 {
    let log_format = ctx.log_format;
    match run_with_inner(ctx, args, detached_run).await {
        Ok(code) => code,
        Err(err) => map_cli_error(log_format, err),
    }
}

async fn run_with_inner(
    ctx: Ctx,
    args: Args,
    detached_run: Option<String>,
) -> Result<i32, CliError> {
    if !std::io::stdin().is_terminal() {
        return Err(CliError::Other(
            "no prompt given. Pass it as an argument or pipe it on stdin:\n  \
             deslicer agent run \"why did plan X fail?\""
                .into(),
        ));
    }

    if let Some(id) = args.conversation.as_deref() {
        parse_conversation_id(id)?;
    }

    let client = AgentClient::from_ctx(&ctx)?;
    let agent_id = match args.agent.as_deref() {
        Some(raw) => Some(resolve_agent(&client, raw).await?),
        None => None,
    };

    run_loop(ctx, client, agent_id, args, detached_run).await
}

async fn run_loop(
    ctx: Ctx,
    client: AgentClient,
    agent_id: Option<String>,
    args: Args,
    mut detached_run: Option<String>,
) -> Result<i32, CliError> {
    let mut conversation_id = args.conversation;

    eprintln!(
        "{}",
        banner(args.agent.as_deref(), conversation_id.as_deref())
    );

    loop {
        let line = match read_prompt()? {
            ReplLine::Empty => continue,
            ReplLine::Exit => return Ok(0),
            ReplLine::Help => {
                eprintln!("{}", help_text());
                continue;
            }
            ReplLine::Prompt(prompt) => prompt,
        };

        if let Some(run_id) = detached_run.as_deref() {
            if run_still_going(&client, run_id).await? {
                eprintln!(
                    "Run {run_id} is still going. Wait, or follow it with \
                     `deslicer agent logs {run_id} --follow`."
                );
                continue;
            }
            detached_run = None;
        }

        let turn = start_and_stream(
            &ctx,
            &client,
            agent_id.as_deref(),
            &line,
            conversation_id.as_deref(),
            args.verbose,
        )
        .await?;

        if conversation_id.is_none() {
            conversation_id = turn.conversation_id.clone();
        }

        if let Some(reason) = turn.failure.as_deref() {
            eprintln!("The run failed: {reason}");
        }

        match turn.end {
            StreamEnd::Completed => {}
            StreamEnd::Cancelled => {
                eprintln!(
                    "{}",
                    cancelled_message(turn.run_id.as_deref(), turn.conversation_id.as_deref())
                );
                detached_run = turn.run_id;
            }
            StreamEnd::Truncated => {
                eprintln!(
                    "The connection closed before the run finished. {}",
                    cancelled_message(turn.run_id.as_deref(), turn.conversation_id.as_deref())
                );
                detached_run = turn.run_id;
            }
        }
    }
}

fn banner(agent_label: Option<&str>, conversation_id: Option<&str>) -> String {
    match (conversation_id, agent_label) {
        (Some(id), _) => format!("Continuing conversation {id}. Type a prompt, /help, or /exit."),
        (None, Some(name)) => format!("{name}. Type a prompt, /help, or /exit."),
        (None, None) => "Orchestrator. Type a prompt, /help, or /exit.".to_string(),
    }
}

fn help_text() -> &'static str {
    "Commands:\n  /help   this list\n  /exit   leave (also /quit or Ctrl-D)\n\n\
     Type anything else as a prompt. Ctrl-C during a run detaches and stays here."
}

fn parse_line(raw: &str) -> ReplLine {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ReplLine::Empty;
    }
    let command = trimmed.split_whitespace().next().unwrap_or("");
    if command.eq_ignore_ascii_case("/exit") || command.eq_ignore_ascii_case("/quit") {
        return ReplLine::Exit;
    }
    if command.eq_ignore_ascii_case("/help") {
        return ReplLine::Help;
    }
    ReplLine::Prompt(trimmed.to_string())
}

fn read_prompt() -> Result<ReplLine, CliError> {
    eprint!("> ");
    io::stderr()
        .flush()
        .map_err(|err| CliError::Other(format!("write prompt: {err}")))?;

    let mut buf = String::new();
    let n = io::stdin()
        .read_line(&mut buf)
        .map_err(|err| CliError::Other(format!("read prompt: {err}")))?;
    if n == 0 {
        eprintln!();
        return Ok(ReplLine::Exit);
    }
    Ok(parse_line(&buf))
}

async fn run_still_going(client: &AgentClient, run_id: &str) -> Result<bool, CliError> {
    let status = client.run_status(run_id).await?;
    Ok(!status.is_terminal())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_lines_are_ignored() {
        assert_eq!(parse_line("  \n"), ReplLine::Empty);
    }

    #[test]
    fn exit_and_quit_are_case_insensitive() {
        assert_eq!(parse_line("/EXIT"), ReplLine::Exit);
        assert_eq!(parse_line("/quit now"), ReplLine::Exit);
    }

    #[test]
    fn help_is_recognised() {
        assert_eq!(parse_line("/help"), ReplLine::Help);
    }

    #[test]
    fn other_text_is_a_prompt() {
        assert_eq!(
            parse_line("  also check the SHC  "),
            ReplLine::Prompt("also check the SHC".into())
        );
    }

    #[test]
    fn a_slash_that_is_not_a_command_is_still_a_prompt() {
        assert_eq!(parse_line("/indexes"), ReplLine::Prompt("/indexes".into()));
    }

    #[test]
    fn banner_names_a_resumed_conversation() {
        let text = banner(None, Some("c-1"));
        assert!(text.contains("c-1"), "{text}");
        assert!(text.contains("/exit"), "{text}");
    }

    #[test]
    fn banner_names_a_chosen_agent() {
        let text = banner(Some("slicer"), None);
        assert!(text.starts_with("slicer."), "{text}");
    }
}
