use clap::{Args as ClapArgs, Subcommand};
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::org::client::{OrgClient, PlanApprovalSettings, PlanExecutionSettings};
use crate::commands::pipeline::map_cli_error;
use crate::Ctx;

#[derive(Subcommand)]
pub enum SettingsCmd {
    /// Show or update plan approval settings
    Approval(ApprovalArgs),
    /// Show or update plan execution settings
    Execution(ExecutionArgs),
}

#[derive(ClapArgs)]
pub struct ApprovalArgs {
    /// Require human approval for plans
    #[arg(long)]
    pub require_human_approval: Option<bool>,

    /// Allow auto-approve (usually false in north-star tenants)
    #[arg(long)]
    pub auto_approve: Option<bool>,

    /// Apply updates. Without this flag (and without update flags), show current settings.
    /// With update flags but without --yes, print a dry-run.
    #[arg(long)]
    pub yes: bool,
}

#[derive(ClapArgs)]
pub struct ExecutionArgs {
    /// Allow CI jobs to execute approved plans
    #[arg(long)]
    pub allow_ci_execute: Option<bool>,

    /// Require signed approve before execute
    #[arg(long)]
    pub require_signed_approve: Option<bool>,

    #[arg(long)]
    pub yes: bool,
}

pub async fn dispatch(ctx: Ctx, cmd: SettingsCmd) -> i32 {
    match cmd {
        SettingsCmd::Approval(args) => approval(ctx, args).await,
        SettingsCmd::Execution(args) => execution(ctx, args).await,
    }
}

async fn approval(ctx: Ctx, args: ApprovalArgs) -> i32 {
    let wants_update = args.require_human_approval.is_some() || args.auto_approve.is_some();
    if wants_update && !args.yes {
        match ctx.log_format {
            LogFormat::Json => println!(
                "{}",
                json!({
                    "dry_run": true,
                    "action": "org_settings_approval",
                    "require_human_approval": args.require_human_approval,
                    "auto_approve": args.auto_approve,
                })
            ),
            LogFormat::Human => {
                println!("dry-run: would update plan approval settings\npass --yes to apply")
            }
        }
        return 0;
    }

    let client = match OrgClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    if wants_update {
        let update = PlanApprovalSettings {
            require_human_approval: args.require_human_approval,
            auto_approve: args.auto_approve,
            approver_roles: None,
            message: None,
        };
        return match client.set_approval_settings(&update).await {
            Ok(settings) => emit_approval(&ctx, &settings),
            Err(err) => map_cli_error(ctx.log_format, err),
        };
    }

    match client.get_approval_settings().await {
        Ok(settings) => emit_approval(&ctx, &settings),
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

async fn execution(ctx: Ctx, args: ExecutionArgs) -> i32 {
    let wants_update = args.allow_ci_execute.is_some() || args.require_signed_approve.is_some();
    if wants_update && !args.yes {
        match ctx.log_format {
            LogFormat::Json => println!(
                "{}",
                json!({
                    "dry_run": true,
                    "action": "org_settings_execution",
                    "allow_ci_execute": args.allow_ci_execute,
                    "require_signed_approve": args.require_signed_approve,
                })
            ),
            LogFormat::Human => {
                println!("dry-run: would update plan execution settings\npass --yes to apply")
            }
        }
        return 0;
    }

    let client = match OrgClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    if wants_update {
        let update = PlanExecutionSettings {
            allow_ci_execute: args.allow_ci_execute,
            require_signed_approve: args.require_signed_approve,
            message: None,
        };
        return match client.set_execution_settings(&update).await {
            Ok(settings) => emit_execution(&ctx, &settings),
            Err(err) => map_cli_error(ctx.log_format, err),
        };
    }

    match client.get_execution_settings().await {
        Ok(settings) => emit_execution(&ctx, &settings),
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn emit_approval(ctx: &Ctx, settings: &PlanApprovalSettings) -> i32 {
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(settings) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize settings: {err}");
                1
            }
        },
        LogFormat::Human => {
            if let Some(v) = settings.require_human_approval {
                println!("require_human_approval: {v}");
            }
            if let Some(v) = settings.auto_approve {
                println!("auto_approve: {v}");
            }
            if let Some(roles) = settings.approver_roles.as_ref() {
                println!("approver_roles: {}", roles.join(", "));
            }
            if let Some(msg) = settings.message.as_deref() {
                println!("{msg}");
            }
            0
        }
    }
}

fn emit_execution(ctx: &Ctx, settings: &PlanExecutionSettings) -> i32 {
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(settings) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize settings: {err}");
                1
            }
        },
        LogFormat::Human => {
            if let Some(v) = settings.allow_ci_execute {
                println!("allow_ci_execute: {v}");
            }
            if let Some(v) = settings.require_signed_approve {
                println!("require_signed_approve: {v}");
            }
            if let Some(msg) = settings.message.as_deref() {
                println!("{msg}");
            }
            0
        }
    }
}
