use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::commands::routine::client::RoutineClient;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Routine id (from `routine list`)
    pub id: String,

    /// Optional model override for this run
    #[arg(long)]
    pub model: Option<String>,

    /// Trigger the run. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    if !args.yes {
        match ctx.log_format {
            LogFormat::Json => {
                let _ = crate::json_doc::print_success(&json!({
                    "dry_run": true,
                    "action": "run",
                    "routine_id": args.id,
                    "model": args.model,
                }));
            }
            LogFormat::Human => {
                let model = args.model.as_deref().unwrap_or("(default)");
                println!(
                    "dry-run: would run routine {} with model {model}\npass --yes to trigger",
                    args.id
                );
            }
        }
        return 0;
    }
    let client = match RoutineClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client.run(&args.id, args.model.as_deref()).await {
        Ok(run) => match ctx.log_format {
            LogFormat::Json => match crate::json_doc::to_pretty_string(&run) {
                Ok(json) => {
                    println!("{json}");
                    0
                }
                Err(err) => {
                    eprintln!("failed to serialize run: {err}");
                    1
                }
            },
            LogFormat::Human => {
                println!(
                    "routine run started: id={} status={}",
                    run.id,
                    run.status.as_deref().unwrap_or("-")
                );
                0
            }
        },
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}
