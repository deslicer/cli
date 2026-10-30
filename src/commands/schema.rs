//! `deslicer schema` — clap-derived JSON self-description (issue #222).

use crate::errors::CliError;
use crate::reporting::emit_cli_error;
use crate::schema::{build_schema, print_schema};
use crate::Ctx;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Restrict output to one command path (e.g. `change plan`)
    #[arg(long = "command")]
    pub command: Option<String>,
}

pub fn run(ctx: Ctx, args: Args) -> i32 {
    match build_schema(args.command.as_deref()) {
        Ok(doc) => match serde_json::to_string_pretty(&doc) {
            Ok(text) => {
                println!("{text}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize schema: {err}");
                1
            }
        },
        Err(message) => emit_cli_error(ctx.log_format, &CliError::InvalidInput(message)),
    }
}

/// Used by `main` for the `--help --json` alias (always JSON on stdout).
pub fn run_help_json_alias(command: Option<&str>) -> i32 {
    print_schema(command)
}
