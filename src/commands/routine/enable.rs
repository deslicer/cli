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

    /// Apply the enable. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    if !args.yes {
        print_dry_run(&ctx, &args.id, "enable");
        return 0;
    }
    let client = match RoutineClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client.set_enabled(&args.id, true).await {
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
                println!("enabled routine {}", args.id);
                0
            }
        },
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn print_dry_run(ctx: &Ctx, id: &str, action: &str) {
    match ctx.log_format {
        LogFormat::Json => {
            let _ = crate::json_doc::print_success(
                &json!({ "dry_run": true, "action": action, "routine_id": id }),
            );
        }
        LogFormat::Human => println!("dry-run: would {action} routine {id}\npass --yes to apply"),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn help_requires_yes() {
        let mut cmd = crate::cli::Cli::command();
        let routine = cmd.find_subcommand_mut("routine").expect("routine");
        let enable = routine.find_subcommand_mut("enable").expect("enable");
        let mut buf = Vec::new();
        enable.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--yes"));
    }
}
