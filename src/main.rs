use anyhow::Result;
use clap::Parser;
use deslicer_cli::cli::Cli;
use deslicer_cli::commands::agent::rewrite_agent_argv;
use deslicer_cli::reporting::{emit_clap_error, log_format_from_args}; // pragma: allowlist secret

#[tokio::main]
async fn main() -> Result<()> {
    let args = rewrite_agent_argv(std::env::args_os());
    let log_format = log_format_from_args(&args);
    match Cli::try_parse_from(&args) {
        Ok(cli) => std::process::exit(cli.run().await),
        Err(err) => std::process::exit(emit_clap_error(log_format, &err)),
    }
}
