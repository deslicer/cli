//! Lists recent CLI agent runs for this session.

use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::errors::CliError;
use crate::Ctx;

use super::client::AgentClient;
use super::types::RunListItem;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    match run_inner(ctx).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: Ctx) -> Result<i32, CliError> {
    let body = AgentClient::from_ctx(&ctx)?
        .list_runs(None, None, None)
        .await?;

    match ctx.log_format {
        LogFormat::Json => {
            let text = serde_json::to_string_pretty(&serde_json::json!({
                "runs": body.runs,
                "nextCursor": body.next_cursor,
            }))
            .map_err(|err| CliError::Other(format!("serialize runs: {err}")))?;
            println!("{text}");
        }
        LogFormat::Human => print!("{}", format_runs_human(&body.runs)),
    }
    Ok(0)
}

fn format_runs_human(runs: &[RunListItem]) -> String {
    if runs.is_empty() {
        return "No runs yet. Start one with `deslicer agent run`.\n".to_string();
    }

    let mut lines = vec!["RUN  STATUS  AGENT  STARTED  PREVIEW".to_string()];
    for run in runs {
        lines.push(format!(
            "{}  {}  {}  {}  {}",
            run.run_id,
            run.status,
            run.agent_id.as_deref().unwrap_or("-"),
            run.started_at,
            run.prompt_preview.as_deref().unwrap_or("-"),
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_row(id: &str, preview: &str) -> RunListItem {
        RunListItem {
            run_id: id.into(),
            status: "succeeded".into(),
            agent_id: Some("agent-1".into()),
            conversation_id: None,
            started_at: "2026-08-31T11:00:00.000Z".into(),
            finished_at: None,
            prompt_preview: Some(preview.into()),
        }
    }

    #[test]
    fn lists_run_id_first_so_it_can_be_copied_into_agent_logs() {
        let text = format_runs_human(&[run_row("r1", "check the fleet")]);
        assert!(text.starts_with("RUN  STATUS  AGENT  STARTED  PREVIEW\nr1  succeeded"));
        assert!(text.contains("check the fleet"));
    }

    #[test]
    fn empty_list_names_the_command_that_starts_a_run() {
        assert!(format_runs_human(&[]).contains("deslicer agent run"));
    }

    #[test]
    fn a_missing_preview_renders_as_a_dash() {
        let mut row = run_row("r1", "x");
        row.prompt_preview = None;
        let text = format_runs_human(&[row]);
        assert!(text.contains("r1  succeeded  agent-1"), "{text}");
        assert!(text.trim_end().ends_with("  -"), "{text}");
    }
}
