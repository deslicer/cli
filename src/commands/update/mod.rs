//! `deslicer update` — self-update from GitHub Releases.
//!
//! Resolves the latest stable tag (or an explicit `--version`), downloads the
//! release archive for the running target triple, verifies the SHA-256
//! sidecar, and atomically replaces the current executable.

use clap::Args as ClapArgs;

mod apply;
mod release;

use crate::errors::CliError;

#[derive(ClapArgs)]
pub struct Args {
    /// Install a specific release tag (e.g. v1.2.0) instead of the latest.
    #[arg(long)]
    pub version: Option<String>,

    /// Only report whether a newer release exists; do not install anything.
    #[arg(long)]
    pub check: bool,
}

pub async fn run(args: Args) -> i32 {
    match run_inner(&args).await {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("update failed: {err}");
            err.exit_code()
        }
    }
}

async fn run_inner(args: &Args) -> Result<(), CliError> {
    let current = env!("CARGO_PKG_VERSION");
    let target_tag = match &args.version {
        Some(tag) => release::validate_tag(tag)?,
        None => release::resolve_latest_tag().await?,
    };

    let target_version = target_tag.trim_start_matches('v');
    if target_version == current {
        println!("deslicer {current} is already up to date");
        return Ok(());
    }

    if args.check {
        println!("update available: {current} -> {target_version}");
        println!("run `deslicer update` to install it");
        return Ok(());
    }

    println!("updating deslicer {current} -> {target_version}");
    apply::download_and_replace(&target_tag).await?;
    println!("updated to deslicer {target_version}");
    Ok(())
}
