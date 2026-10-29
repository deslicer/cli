use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Splunk package name to upgrade
    #[arg(long, required = true)]
    pub package: String,

    /// Target package version
    #[arg(long, required = true)]
    pub target_version: String,

    /// Optional inventory group targets (comma-separated)
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub targets: Vec<String>,

    #[arg(long)]
    pub name: Option<String>,

    #[arg(long)]
    pub environment: Option<String>,

    /// Create the Pending upgrade plan. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    if !args.yes {
        match ctx.log_format {
            LogFormat::Json => println!(
                "{}",
                json!({
                    "dry_run": true,
                    "action": "upgrade_plan",
                    "package": args.package,
                    "target_version": args.target_version,
                    "targets": args.targets,
                    "will_auto_approve": false,
                })
            ),
            LogFormat::Human => println!(
                "dry-run: would create a Pending Splunk upgrade plan (package {}) → {}\n\
                 check `deslicer upgrade status` for flapping-gate; pass --yes to create, then `deslicer change approve`",
                args.package, args.target_version
            ),
        }
        return 0;
    }

    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client
        .create_upgrade_plan(
            Some(&args.package),
            Some(&args.target_version),
            &args.targets,
            args.name.as_deref(),
        )
        .await
    {
        Ok(plan) => {
            eprintln!("upgrade plan created (Pending — approve before deploy)");
            emit_change_plan(&plan)
        }
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use crate::cli::Cli;

    #[test]
    fn help_requires_package_target_and_yes() {
        let mut cmd = crate::cli::Cli::command();
        let upgrade = cmd.find_subcommand_mut("upgrade").expect("upgrade");
        let plan = upgrade.find_subcommand_mut("plan").expect("plan");
        let mut buf = Vec::new();
        plan.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--package"));
        assert!(help.contains("--target-version"));
        assert!(help.contains("--yes"));
        assert!(help.contains("Pending") || help.contains("flapping"));
    }

    #[test]
    fn missing_package_and_version_is_usage_error() {
        let Err(err) = Cli::try_parse_from(["deslicer", "upgrade", "plan"]) else {
            panic!("expected usage error");
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn package_without_target_version_is_usage_error() {
        let Err(err) = Cli::try_parse_from(["deslicer", "upgrade", "plan", "--package", "splunk"])
        else {
            panic!("expected usage error");
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn required_args_parse_for_dry_run() {
        let cli = Cli::try_parse_from([
            "deslicer",
            "upgrade",
            "plan",
            "--package",
            "splunk",
            "--target-version",
            "9.3.0",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Upgrade(crate::commands::upgrade::UpgradeCmd::Plan(args)) => {
                assert_eq!(args.package, "splunk");
                assert_eq!(args.target_version, "9.3.0");
                assert!(!args.yes);
            }
            _ => panic!("expected upgrade plan"),
        }
    }
}
