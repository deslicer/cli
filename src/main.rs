use anyhow::Result;
use clap::Parser;
use deslicer_cli::cli::{Cli, LogFormat};
use deslicer_cli::commands::agent::rewrite_agent_argv;
use deslicer_cli::commands::schema::run_help_json_alias;
use deslicer_cli::reporting::{emit_clap_error, emit_output_conflict, output_format_from_args}; // pragma: allowlist secret
use deslicer_cli::schema::{help_json_command_path, wants_help_json};

#[tokio::main]
async fn main() -> Result<()> {
    let args = rewrite_agent_argv(std::env::args_os());
    let args_str: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // `--help --json` (and `--output json … --help`) is an alias for `deslicer schema`.
    if wants_help_json(&args_str) {
        let path = help_json_command_path(&args_str);
        std::process::exit(run_help_json_alias(path.as_deref()));
    }
    let log_format = match output_format_from_args(&args) {
        Ok(format) => format,
        Err(_) => {
            std::process::exit(emit_output_conflict(LogFormat::Json));
        }
    };
    match Cli::try_parse_from(&args) {
        Ok(cli) => std::process::exit(cli.run().await),
        Err(err) => std::process::exit(emit_clap_error(log_format, &err)),
    }
}
