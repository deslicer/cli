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

    /// Command path as positional segments (e.g. `deslicer schema change plan`)
    #[arg(value_name = "COMMAND", num_args = 0.., trailing_var_arg = true)]
    pub path: Vec<String>,
}

impl Args {
    /// Resolve `--command` and/or positional path segments into one filter.
    fn command_filter(&self) -> Result<Option<String>, String> {
        let from_flag = self
            .command
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let from_path = if self.path.is_empty() {
            None
        } else {
            Some(self.path.join(" "))
        };
        match (from_flag, from_path) {
            (Some(a), Some(b)) if a.split_whitespace().eq(b.split_whitespace()) => Ok(Some(a)),
            (Some(a), Some(b)) => Err(format!(
                "conflicting command filters: --command {a:?} and positional path {b:?}"
            )),
            (Some(a), None) | (None, Some(a)) => Ok(Some(a)),
            (None, None) => Ok(None),
        }
    }
}

pub fn run(ctx: Ctx, args: Args) -> i32 {
    let filter = match args.command_filter() {
        Ok(filter) => filter,
        Err(message) => {
            return emit_cli_error(ctx.log_format, &CliError::InvalidInput(message));
        }
    };
    match build_schema(filter.as_deref()) {
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
