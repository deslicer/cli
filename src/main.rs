use anyhow::Result;
use clap::Parser;
use deslicer_cli::cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    std::process::exit(cli.run().await);
}
