//! `deslicer upgrade readiness | preview | plan` (refs #149).
//!
//! readiness and preview are read-only. plan creates a Pending plan only;
//! approval and deploy stay with `deslicer change approve` / `change deploy`.

use clap::{Args as ClapArgs, Subcommand};

use crate::cli::LogFormat;
use crate::observer_client::{UPGRADE_FAMILIES, UPGRADE_ROLE_GROUPS};
use crate::Ctx;

mod plan;
mod preview;
mod readiness;

#[derive(Subcommand)]
pub enum UpgradeCmd {
    /// Read-only upgrade readiness for one host (path, pin, captain stability)
    Readiness(readiness::Args),
    /// Read-only rolling upgrade wave preview (no plan is created)
    Preview(RollingArgs),
    /// Deprecated alias for `upgrade preview` (removed next release)
    #[command(hide = true)]
    Status(RollingArgs),
    /// Create a Pending rolling upgrade plan (never approves or deploys)
    Plan(RollingArgs),
}

/// Arguments shared by `preview` and `plan` (same Observer request body).
#[derive(ClapArgs, Debug, Clone)]
pub struct RollingArgs {
    /// Upgrade family
    #[arg(long, required = true, value_parser = clap::builder::PossibleValuesParser::new(UPGRADE_FAMILIES))]
    pub family: String,

    /// Host to upgrade (hostname or host UUID). Repeatable.
    #[arg(long = "target", required = true, num_args = 1)]
    pub targets: Vec<String>,

    /// Splunk version to upgrade to (e.g. 9.4.1)
    #[arg(long, required = true)]
    pub to_version: String,

    /// Current SHC captain hostname when Insights has not reported one
    #[arg(long)]
    pub captain: Option<String>,

    /// Enable only these role groups, in order. Repeatable. Default: full catalog.
    #[arg(long = "role-group", num_args = 1, value_parser = clap::builder::PossibleValuesParser::new(UPGRADE_ROLE_GROUPS))]
    pub role_groups: Vec<String>,

    #[arg(long)]
    pub environment: Option<String>,

    /// Print JSON (same as --log-format json)
    #[arg(long)]
    pub json: bool,
}

pub(crate) fn output_format(ctx: &Ctx, json: bool) -> LogFormat {
    if json {
        LogFormat::Json
    } else {
        ctx.log_format
    }
}

pub(crate) fn validate_version(raw: &str) -> Result<String, crate::errors::CliError> {
    let v = raw.trim();
    let ok = !v.is_empty()
        && v.len() <= 32
        && v.split('.').count() >= 2
        && v.split('.')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if ok {
        Ok(v.to_string())
    } else {
        Err(crate::errors::CliError::InvalidInput(format!(
            "--to-version must look like 9.4.1 (got {raw:?})"
        )))
    }
}

pub(crate) async fn rolling_request(
    client: &crate::observer_client::Client,
    args: &RollingArgs,
) -> Result<crate::observer_client::UpgradeRollingRequest, crate::errors::CliError> {
    let to_version = validate_version(&args.to_version)?;
    let hosts = client.list_enrolled_hosts().await?;
    let host_ids = crate::observer_client::resolve_targets(&hosts, &args.targets)?;
    Ok(crate::observer_client::UpgradeRollingRequest {
        family: args.family.clone(),
        host_ids,
        to_version: Some(to_version),
        captain_hostname: args.captain.clone(),
        enabled_role_groups: args.role_groups.clone(),
    })
}

pub(crate) fn print_json(value: &impl serde::Serialize) -> i32 {
    crate::json_doc::print_success(value)
}

pub async fn dispatch(ctx: Ctx, cmd: UpgradeCmd) -> i32 {
    match cmd {
        UpgradeCmd::Readiness(args) => readiness::run(ctx, args).await,
        UpgradeCmd::Preview(args) => preview::run(ctx, args).await,
        UpgradeCmd::Status(args) => {
            eprintln!(
                "warning: `deslicer upgrade status` is deprecated and will be removed in the next release; use `deslicer upgrade preview`"
            );
            preview::run(ctx, args).await
        }
        UpgradeCmd::Plan(args) => plan::run(ctx, args).await,
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::{Cli, Command};

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut v = vec!["deslicer", "upgrade"];
        v.extend_from_slice(args);
        Cli::try_parse_from(v)
    }

    #[test]
    fn plan_parses_repeatable_targets_and_role_groups() {
        let cli = parse(&[
            "plan",
            "--family",
            "upgrade_rolling_clusters",
            "--target",
            "sh1",
            "--target",
            "sh2",
            "--to-version",
            "9.4.1",
            "--captain",
            "sh1",
            "--role-group",
            "precheck",
            "--role-group",
            "searchhead",
            "--json",
        ])
        .unwrap();
        let Command::Upgrade(UpgradeCmd::Plan(a)) = cli.command else {
            panic!()
        };
        assert_eq!(a.targets, vec!["sh1", "sh2"]);
        assert_eq!(a.role_groups, vec!["precheck", "searchhead"]);
        assert!(a.json);
    }

    #[test]
    fn missing_or_invalid_args_are_usage_errors() {
        for args in [
            vec![
                "plan",
                "--family",
                "upgrade_non_clustered",
                "--to-version",
                "9.4.1",
            ],
            vec!["plan", "--target", "h", "--to-version", "9.4.1"],
            vec![
                "plan",
                "--family",
                "bogus",
                "--target",
                "h",
                "--to-version",
                "9.4.1",
            ],
            vec![
                "preview",
                "--family",
                "upgrade_non_clustered",
                "--target",
                "h",
                "--to-version",
                "9.4.1",
                "--role-group",
                "x",
            ],
            vec!["readiness"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn plan_has_no_apply_flag() {
        for flag in ["--apply", "--yes", "--deploy", "--approve"] {
            assert!(parse(&[
                "plan",
                "--family",
                "upgrade_non_clustered",
                "--target",
                "h",
                "--to-version",
                "9.4.1",
                flag
            ])
            .is_err());
        }
    }

    #[test]
    fn status_is_a_hidden_preview_alias() {
        let cli = parse(&[
            "status",
            "--family",
            "upgrade_non_clustered",
            "--target",
            "h",
            "--to-version",
            "9.4.1",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Upgrade(UpgradeCmd::Status(_))
        ));
    }

    #[test]
    fn version_validation() {
        assert!(validate_version("9.4.1").is_ok());
        assert!(validate_version("9.4").is_ok());
        assert!(validate_version("latest").is_err());
        assert!(validate_version("9..1").is_err());
    }
}
