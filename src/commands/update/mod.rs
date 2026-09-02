//! `[REDACTED] update` — self-update from GitHub Releases.
//!
//! Resolves the latest stable tag (or an explicit `--version`), downloads the
//! release archive for the running target triple, verifies the SHA-256
//! sidecar, and atomically replaces the current executable.

use clap::Args as ClapArgs;
use serde_json::json;

mod apply;
mod release;

use crate::cli::LogFormat;
use crate::commands::auth::format::print_output;
use crate::errors::CliError;
use crate::reporting::emit_cli_error;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Install a specific release tag (e.g. v1.2.0) instead of the latest.
    #[arg(long)]
    pub version: Option<String>,

    /// Only report whether a newer release exists; do not install anything.
    #[arg(long)]
    pub check: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(&ctx, &args).await {
        Ok(()) => 0,
        Err(err) => emit_cli_error(ctx.log_format, &err),
    }
}

async fn run_inner(ctx: &Ctx, args: &Args) -> Result<(), CliError> {
    let current = env!("CARGO_PKG_VERSION");
    let target_tag = match &args.version {
        Some(tag) => release::validate_tag(tag)?,
        None => release::resolve_latest_tag().await?,
    };

    let target_version = target_tag.trim_start_matches('v');
    let up_to_date = target_version == current;

    if args.check {
        let payload = json!({
            "current": current,
            "latest": target_version,
            "up_to_date": up_to_date,
        });
        let human = if up_to_date {
            format!("[REDACTED] {current} is already up to date\n")
        } else {
            format!(
                "update available: {current} -> {target_version}\nrun `[REDACTED] update` to install it\n"
            )
        };
        print_output(ctx.log_format, &payload, &human);
        return Ok(());
    }

    if up_to_date {
        print_output(
            ctx.log_format,
            &json!({
                "current": current,
                "latest": target_version,
                "up_to_date": true,
            }),
            &format!("[REDACTED] {current} is already up to date\n"),
        );
        return Ok(());
    }

    let human = format!("updating [REDACTED] {current} -> {target_version}\n");
    if ctx.log_format == LogFormat::Human {
        print!("{human}");
    }
    apply::download_and_replace(&target_tag).await?;
    let done = format!("updated to [REDACTED] {target_version}\n");
    if ctx.log_format == LogFormat::Human {
        print!("{done}");
    } else {
        print_output(
            ctx.log_format,
            &json!({
                "current": target_version,
                "latest": target_version,
                "up_to_date": true,
                "updated": true,
            }),
            &done,
        );
    }
    Ok(())
}
