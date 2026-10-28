use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::commands::routine::client::{RoutineClient, RoutineRun};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Routine id (from `routine list`)
    pub id: String,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let client = match RoutineClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let runs = match client.list_runs(&args.id).await {
        Ok(runs) => runs,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&runs) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize runs: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_runs_human(&runs));
            0
        }
    }
}

fn format_runs_human(runs: &[RoutineRun]) -> String {
    if runs.is_empty() {
        return "No runs for this routine.\n".to_string();
    }
    let mut lines = vec!["ID  STATUS  MODEL  STARTED  FINISHED".to_string()];
    for r in runs {
        lines.push(format!(
            "{}  {}  {}  {}  {}",
            r.id,
            r.status.as_deref().unwrap_or("-"),
            r.model.as_deref().unwrap_or("-"),
            r.started_at.as_deref().unwrap_or("-"),
            r.finished_at.as_deref().unwrap_or("-")
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}
