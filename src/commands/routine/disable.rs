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

    /// Apply the disable. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    if !args.yes {
        match ctx.log_format {
            LogFormat::Json => {
                let _ = crate::json_doc::print_success(
                    &json!({ "dry_run": true, "action": "disable", "routine_id": args.id }),
                );
            }
            LogFormat::Human => {
                println!(
                    "dry-run: would disable routine {}\npass --yes to apply",
                    args.id
                )
            }
        }
        return 0;
    }
    let client = match RoutineClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client.set_enabled(&args.id, false).await {
        Ok(routine) => match ctx.log_format {
            LogFormat::Json => match crate::json_doc::to_pretty_string(&routine) {
                Ok(json) => {
                    println!("{json}");
                    0
                }
                Err(err) => {
                    eprintln!("failed to serialize routine: {err}");
                    1
                }
            },
            LogFormat::Human => {
                println!("disabled routine {}", args.id);
                0
            }
        },
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}
