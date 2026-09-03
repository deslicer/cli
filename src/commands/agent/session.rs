//! Starts one agent run and drains its stream.

use uuid::Uuid;

use crate::cli::LogFormat;
use crate::errors::CliError;
use crate::Ctx;

use super::client::AgentClient;
use super::render::{RenderMode, Renderer};
use super::stream::{consume_stream, StreamEnd};

/// Exit code convention for "interrupted by SIGINT" (128 + 2).
pub const EXIT_INTERRUPTED: i32 = 130;

/// One prompt's server run, after its stream has been read or detached.
pub struct Turn {
    pub run_id: Option<String>,
    pub conversation_id: Option<String>,
    pub end: StreamEnd,
    pub failure: Option<String>,
}

/// Opens a run and streams it until it finishes or the reader hangs up.
pub async fn start_and_stream(
    ctx: &Ctx,
    client: &AgentClient,
    agent_id: Option<&str>,
    prompt: &str,
    conversation_id: Option<&str>,
    verbose: bool,
) -> Result<Turn, CliError> {
    let idempotency_key = Uuid::new_v4().to_string();
    let started = client
        .start_run(agent_id, prompt, conversation_id, &idempotency_key)
        .await?;

    let run_id = started.run_id.clone();
    let started_conversation = started.conversation_id.clone();
    announce_turn(
        ctx,
        conversation_id.is_none(),
        started_conversation.as_deref(),
        run_id.as_deref(),
        verbose,
    );

    let (end, failure) = drain_turn(ctx, started.response, verbose).await?;

    Ok(Turn {
        run_id,
        conversation_id: started_conversation,
        end,
        failure,
    })
}

fn announce_turn(
    ctx: &Ctx,
    is_new_conversation: bool,
    conversation_id: Option<&str>,
    run_id: Option<&str>,
    verbose: bool,
) {
    if ctx.log_format != LogFormat::Human {
        return;
    }
    if let Some(notice) = conversation_notice(is_new_conversation, conversation_id) {
        eprintln!("{notice}");
    }
    if verbose {
        if let Some(id) = run_id {
            eprintln!("Run {id}");
        }
    }
}

async fn drain_turn(
    ctx: &Ctx,
    response: reqwest::Response,
    verbose: bool,
) -> Result<(StreamEnd, Option<String>), CliError> {
    let mode = match ctx.log_format {
        LogFormat::Human => RenderMode::Human,
        LogFormat::Json => RenderMode::Json,
    };
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut renderer = Renderer::new(stdout.lock(), stderr.lock(), mode, verbose);

    let streamed = consume_stream(response, &mut renderer, interrupted()).await;
    let finish = renderer.finish();
    let renderer_failure = renderer.failure().map(str::to_string);
    finish?;

    // A read failure must not kill a conversation that already started.
    // The run continues server-side; the REPL can ask another question.
    Ok(match streamed {
        Ok(end) => (end, renderer_failure),
        Err(err) => (
            StreamEnd::Truncated,
            renderer_failure.or_else(|| Some(stream_failure_message(&err))),
        ),
    })
}

fn conversation_notice(is_new: bool, conversation_id: Option<&str>) -> Option<String> {
    if !is_new {
        return None;
    }
    conversation_id.map(|id| format!("Started conversation {id}"))
}

fn stream_failure_message(err: &CliError) -> String {
    match err {
        CliError::Transport(msg) => msg.clone(),
        other => other.to_string(),
    }
}

pub async fn interrupted() {
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
}

pub fn resume_hint(run_id: Option<&str>, conversation_id: Option<&str>) -> Option<String> {
    if let Some(id) = run_id {
        return Some(format!(
            "follow it with `deslicer agent logs {id} --follow`"
        ));
    }
    conversation_id.map(|id| format!("see conversation {id} in the portal"))
}

pub fn cancelled_message(run_id: Option<&str>, conversation_id: Option<&str>) -> String {
    match resume_hint(run_id, conversation_id) {
        Some(hint) => format!("Interrupted. The run continues server-side; {hint}."),
        None => "Interrupted. The run continues server-side.".to_string(),
    }
}

pub fn truncated_message(run_id: Option<&str>, conversation_id: Option<&str>) -> String {
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
    fn cancelled_message_hands_back_the_command_that_reattaches() {
        let text = cancelled_message(Some("r-1"), Some("c-1"));
        assert!(text.contains("deslicer agent logs r-1 --follow"), "{text}");
    }

    #[test]
    fn cancelled_message_never_suggests_the_conversation_flag() {
        assert!(!cancelled_message(Some("r-1"), Some("c-1")).contains("--conversation"));
        assert!(!cancelled_message(None, Some("c-1")).contains("--conversation"));
    }

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

    #[test]
    fn conversation_notice_prints_only_on_the_first_turn() {
        assert_eq!(
            conversation_notice(true, Some("c-1")).as_deref(),
            Some("Started conversation c-1")
        );
        assert_eq!(conversation_notice(false, Some("c-1")), None);
        assert_eq!(conversation_notice(true, None), None);
    }

    #[test]
    fn stream_failure_drops_the_transport_prefix() {
        let err = CliError::Transport("read agent stream: connection reset".into());
        assert_eq!(
            stream_failure_message(&err),
            "read agent stream: connection reset"
        );
    }
}
