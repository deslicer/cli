//! `deslicer update` — self-update from GitHub Releases. // pragma: allowlist secret
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

/// Binary name shown in human-facing update output (must match the release artifact).
pub(crate) const BINARY_NAME: &str = "deslicer"; // pragma: allowlist secret

pub(crate) fn human_already_up_to_date(current: &str) -> String {
    format!("{BINARY_NAME} {current} is already up to date\n")
}

pub(crate) fn human_update_available(current: &str, target_version: &str) -> String {
    format!(
        "update available: {current} -> {target_version}\nrun `{BINARY_NAME} update` to install it\n"
    )
}

pub(crate) fn human_updating(current: &str, target_version: &str) -> String {
    format!("updating {BINARY_NAME} {current} -> {target_version}\n")
}

pub(crate) fn human_updated(target_version: &str) -> String {
    format!("updated to {BINARY_NAME} {target_version}\n")
}

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
            human_already_up_to_date(current)
        } else {
            human_update_available(current, target_version)
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
            &human_already_up_to_date(current),
        );
        return Ok(());
    }

    let human = human_updating(current, target_version);
    if ctx.log_format == LogFormat::Human {
        print!("{human}");
    }
    apply::download_and_replace(&target_tag).await?;
    let done = human_updated(target_version);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scrub_placeholder_literal() -> String {
        ['[', 'R', 'E', 'D', 'A', 'C', 'T', 'E', 'D', ']'].iter().collect()
    }

    #[test]
    fn human_messages_use_binary_name_not_redacted_placeholder() {
        let bad = scrub_placeholder_literal();
        let samples = [
            human_already_up_to_date("1.7.0"),
            human_update_available("1.6.0", "1.7.0"),
            human_updating("1.6.0", "1.7.0"),
            human_updated("1.7.0"),
        ];
        for text in samples {
            assert!(
                text.contains(BINARY_NAME),
                "expected binary name in: {text:?}"
            );
            assert!(
                !text.contains(&bad),
                "scrub placeholder must not appear in: {text:?}"
            );
        }
    }

    #[test]
    fn human_already_up_to_date_matches_expected_shape() {
        assert_eq!(
            human_already_up_to_date("1.7.0"),
            format!("{BINARY_NAME} 1.7.0 is already up to date\n")
        );
    }

    #[test]
    fn human_update_available_includes_install_hint() {
        let text = human_update_available("1.6.0", "1.7.0");
        assert!(text.contains("update available: 1.6.0 -> 1.7.0"));
        assert!(text.contains(&format!("run `{BINARY_NAME} update` to install it")));
    }
}
