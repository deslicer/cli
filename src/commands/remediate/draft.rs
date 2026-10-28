use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::output::emit_change_plan;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub host: Option<String>,

    #[arg(long)]
    pub app: Option<String>,

    #[arg(long)]
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
                "dry-run: would create a draft remediation plan\
                 {}\
                 {}\n\
                 pass --yes to create; output is a draft plan id (never auto-approved)",
                args.host
                    .as_deref()
                    .map(|h| format!(" host={h}"))
                    .unwrap_or_default(),
                args.app
                    .as_deref()
                    .map(|a| format!(" app={a}"))
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
            args.app.as_deref(),
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
    use clap::CommandFactory;

    #[test]
    fn help_requires_yes() {
        let mut cmd = crate::cli::Cli::command();
        let remediate = cmd.find_subcommand_mut("remediate").expect("remediate");
        let draft = remediate.find_subcommand_mut("draft").expect("draft");
        let mut buf = Vec::new();
        draft.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--yes"));
        assert!(help.contains("draft"));
    }
}
