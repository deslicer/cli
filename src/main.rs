use anyhow::Result;
use clap::Parser;
use deslicer_cli::cli::Cli;
use deslicer_cli::commands::agent::rewrite_agent_argv;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse_from(rewrite_agent_argv(std::env::args_os()));
    std::process::exit(cli.run().await);
}
