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
    let conversation_id = started.conversation_id.clone();

    if ctx.log_format == LogFormat::Human {
        if let Some(id) = conversation_id.as_deref() {
            eprintln!("Conversation {id}");
        }
        if verbose {
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
    let mut renderer = Renderer::new(stdout.lock(), stderr.lock(), mode, verbose);

    let end = consume_stream(started.response, &mut renderer, interrupted()).await;
    let finish = renderer.finish();
    let failure = renderer.failure().map(str::to_string);
    let end = end?;
    finish?;

    Ok(Turn {
        run_id,
        conversation_id,
        end,
        failure,
    })
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
}
