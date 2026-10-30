use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::commands::routine::client::RoutineClient;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Routine run id (from `routine runs`)
    pub run_id: String,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let client = match RoutineClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let report = match client.report(&args.run_id).await {
        Ok(report) => report,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(&report) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize report: {err}");
                1
            }
        },
        LogFormat::Human => {
            println!("routine run {}", report.run_id);
            if let Some(status) = report.status.as_deref() {
                println!("status: {status}");
            }
            if let Some(summary) = report.summary.as_deref() {
                println!("summary: {summary}");
            }
            if let Some(body) = report.report.as_ref() {
                match crate::json_doc::to_pretty_string(body) {
                    Ok(json) => println!("{json}"),
                    Err(err) => {
                        eprintln!("failed to format report body: {err}");
                        return 1;
                    }
                }
            }
            0
        }
    }
}
