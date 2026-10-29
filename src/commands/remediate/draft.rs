use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Host where the app should be remediated
    #[arg(long, required_unless_present = "group")]
    pub host: Option<String>,

    /// App to remediate
    #[arg(long, required = true)]
    pub app: String,

    /// Inventory group scope (alternative to --host)
    #[arg(long, required_unless_present = "host")]
    pub group: Option<String>,

    #[arg(long)]
    pub name: Option<String>,

    #[arg(long)]
    pub environment: Option<String>,

    /// Create the draft plan. Without this flag, print a dry-run and exit.
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
                    "action": "remediate_draft",
                    "host": args.host,
                    "app": args.app,
                    "group": args.group,
                    "will_auto_approve": false,
                })
            ),
            LogFormat::Human => println!(
                "dry-run: would create a draft remediation plan app={}\
                 {}\
                 {}\n\
                 pass --yes to create; output is a draft plan id (never auto-approved)",
                args.app,
                args.host
                    .as_deref()
                    .map(|h| format!(" host={h}"))
                    .unwrap_or_default(),
                args.group
                    .as_deref()
                    .map(|g| format!(" group={g}"))
                    .unwrap_or_default()
            ),
        }
        return 0;
    }

    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client
        .remediate_draft(
            args.host.as_deref(),
            Some(&args.app),
            args.group.as_deref(),
            args.name.as_deref(),
        )
        .await
    {
        Ok(draft) => {
            if let Some(plan) = draft.plan.as_ref() {
                eprintln!("remediate draft plan id: {}", plan.external_id());
                return emit_change_plan(plan);
            }
            match ctx.log_format {
                LogFormat::Json => match serde_json::to_string_pretty(&draft) {
                    Ok(json) => {
                        println!("{json}");
                        0
                    }
                    Err(err) => {
                        eprintln!("failed to serialize draft: {err}");
                        1
                    }
                },
                LogFormat::Human => {
                    if let Some(id) = draft.plan_id.as_deref() {
                        println!("draft plan id: {id}");
                    } else {
                        println!("draft created (no plan id in response)");
                    }
                    if let Some(msg) = draft.message.as_deref() {
                        println!("{msg}");
                    }
                    0
                }
            }
        }
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use crate::cli::Cli;

    #[test]
    fn help_requires_app_scope_and_yes() {
        let mut cmd = crate::cli::Cli::command();
        let remediate = cmd.find_subcommand_mut("remediate").expect("remediate");
        let draft = remediate.find_subcommand_mut("draft").expect("draft");
        let mut buf = Vec::new();
        draft.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--app"));
        assert!(help.contains("--host") || help.contains("--group"));
        assert!(help.contains("--yes"));
        assert!(help.contains("draft"));
    }

    #[test]
    fn missing_args_is_usage_error() {
        let Err(err) = Cli::try_parse_from(["deslicer", "remediate", "draft"]) else {
            panic!("expected usage error");
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn app_without_host_or_group_is_usage_error() {
        let Err(err) = Cli::try_parse_from(["deslicer", "remediate", "draft", "--app", "ta_nix"])
        else {
            panic!("expected usage error");
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn app_and_host_parse_for_dry_run() {
        let cli = Cli::try_parse_from([
            "deslicer",
            "remediate",
            "draft",
            "--app",
            "ta_nix",
            "--host",
            "idx1",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Remediate(crate::commands::remediate::RemediateCmd::Draft(
                args,
            )) => {
                assert_eq!(args.app, "ta_nix");
                assert_eq!(args.host.as_deref(), Some("idx1"));
                assert!(!args.yes);
            }
            _ => panic!("expected remediate draft"),
        }
    }
}
