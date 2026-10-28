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

pub(crate) fn human_ahead_of_latest(current: &str, latest: &str) -> String {
    format!(
        "{BINARY_NAME} {current} is newer than the latest release ({latest})\n\
         no update applied; pass --version to install a specific tag\n"
    )
}

pub(crate) fn human_updating(current: &str, target_version: &str) -> String {
    format!("updating {BINARY_NAME} {current} -> {target_version}\n")
}

pub(crate) fn human_updated(target_version: &str) -> String {
    format!("updated to {BINARY_NAME} {target_version}\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VersionRelation {
    Equal,
    UpdateAvailable,
    Ahead,
}

/// Compare dotted semver cores (`X.Y.Z`, optional `-prerelease` / `+build` ignored for order).
pub(crate) fn compare_versions(current: &str, target: &str) -> Result<VersionRelation, CliError> {
    let current_v = parse_semver_core(current)?;
    let target_v = parse_semver_core(target)?;
    Ok(match current_v.cmp(&target_v) {
        std::cmp::Ordering::Equal => VersionRelation::Equal,
        std::cmp::Ordering::Less => VersionRelation::UpdateAvailable,
        std::cmp::Ordering::Greater => VersionRelation::Ahead,
    })
}

fn parse_semver_core(version: &str) -> Result<(u64, u64, u64), CliError> {
    let core = version
        .split_once(['-', '+'])
        .map(|(core, _)| core)
        .unwrap_or(version);
    let mut parts = core.split('.');
    let major = parts
        .next()
        .and_then(|p| p.parse::<u64>().ok())
        .ok_or_else(|| invalid_version(version))?;
    let minor = parts
        .next()
        .and_then(|p| p.parse::<u64>().ok())
        .ok_or_else(|| invalid_version(version))?;
    let patch = parts
        .next()
        .and_then(|p| p.parse::<u64>().ok())
        .ok_or_else(|| invalid_version(version))?;
    if parts.next().is_some() {
        return Err(invalid_version(version));
    }
    Ok((major, minor, patch))
}

fn invalid_version(version: &str) -> CliError {
    CliError::Other(format!(
        "invalid version {version:?}: expected X.Y.Z (optional -prerelease)"
    ))
}

fn relation_status(relation: VersionRelation) -> &'static str {
    match relation {
        VersionRelation::Equal => "up_to_date",
        VersionRelation::UpdateAvailable => "update_available",
        VersionRelation::Ahead => "ahead",
    }
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
    let explicit_version = args.version.is_some();
    let target_tag = match &args.version {
        Some(tag) => release::validate_tag(tag)?,
        None => release::resolve_latest_tag().await?,
    };

    let target_version = target_tag.trim_start_matches('v');
    let relation = compare_versions(current, target_version)?;
    // Explicit --version may install an older tag on purpose. Latest resolution
    // never downgrades a newer local build.
    let should_install = match relation {
        VersionRelation::UpdateAvailable => true,
        VersionRelation::Equal => false,
        VersionRelation::Ahead => explicit_version,
    };

    if args.check {
        let up_to_date = !matches!(relation, VersionRelation::UpdateAvailable);
        let payload = json!({
            "current": current,
            "latest": target_version,
            "up_to_date": up_to_date,
            "status": relation_status(relation),
        });
        let human = match relation {
            VersionRelation::Equal => human_already_up_to_date(current),
            VersionRelation::UpdateAvailable => human_update_available(current, target_version),
            VersionRelation::Ahead => human_ahead_of_latest(current, target_version),
        };
        print_output(ctx.log_format, &payload, &human);
        return Ok(());
    }

    if !should_install {
        let human = match relation {
            VersionRelation::Ahead => human_ahead_of_latest(current, target_version),
            VersionRelation::Equal | VersionRelation::UpdateAvailable => {
                human_already_up_to_date(current)
            }
        };
        print_output(
            ctx.log_format,
            &json!({
                "current": current,
                "latest": target_version,
                "up_to_date": true,
                "status": relation_status(relation),
            }),
            &human,
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
                "status": "up_to_date",
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
        ['[', 'R', 'E', 'D', 'A', 'C', 'T', 'E', 'D', ']']
            .iter()
            .collect()
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

    #[test]
    fn human_ahead_of_latest_does_not_offer_downgrade() {
        let text = human_ahead_of_latest("1.7.1", "1.7.0");
        assert!(text.contains("newer than the latest release (1.7.0)"));
        assert!(!text.contains("update available"));
        assert!(text.contains("--version"));
    }

    #[test]
    fn compare_versions_orders_semver_cores() {
        assert_eq!(
            compare_versions("1.6.0", "1.7.0").unwrap(),
            VersionRelation::UpdateAvailable
        );
        assert_eq!(
            compare_versions("1.7.0", "1.7.0").unwrap(),
            VersionRelation::Equal
        );
        assert_eq!(
            compare_versions("1.7.1", "1.7.0").unwrap(),
            VersionRelation::Ahead
        );
        assert_eq!(
            compare_versions("1.7.1-rc.1", "1.7.0").unwrap(),
            VersionRelation::Ahead
        );
    }
}
