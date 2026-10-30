use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::commands::routine::client::{RoutineClient, RoutineRun};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Routine id (from `routine list`)
    pub id: String,

    #[command(flatten)]
    pub page: crate::pagination::PageArgs,
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
    let page = crate::pagination::page_slice(&runs, &args.page);
    match ctx.log_format {
        LogFormat::Json => crate::pagination::print_page_json(&page),
        LogFormat::Human => {
            print!("{}", format_runs_human(&page.items));
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
