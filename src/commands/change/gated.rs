//! Splunk-changing verbs that used to live outside `change` (#187).
//!
//! These create a Pending plan or write Splunk state. They never approve or
//! deploy. The old command paths remain as deprecated aliases.

use clap::Subcommand;

use crate::Ctx;

#[derive(Subcommand)]
pub enum UpgradeCmd {
    /// Create a Pending rolling upgrade plan (never approves or deploys).
    Plan(crate::commands::upgrade::RollingArgs),
}

#[derive(Subcommand)]
pub enum CatalogCmd {
    /// Create Pending plan(s) from a blueprint (dry-run unless --yes).
    Plan(crate::commands::catalog::plan::Args),
}

#[derive(Subcommand)]
pub enum RemediateCmd {
    /// Create a draft remediation plan (dry-run unless --yes).
    Draft(crate::commands::remediate::draft::Args),
}

#[derive(Subcommand)]
pub enum AppCmd {
    /// Adopt a live app as Git-managed (dry-run unless --yes).
    Adopt(crate::commands::inventory::apps::AdoptArgs),
}

#[derive(Subcommand)]
pub enum MappingCmd {
    /// Upsert one environment app mapping (dry-run unless --yes).
    Set(crate::commands::environments::mappings::SetArgs),
    /// Adopt an app into the environment mapping (dry-run unless --yes).
    Adopt(crate::commands::environments::mappings::AdoptArgs),
}

pub async fn dispatch_upgrade(ctx: Ctx, cmd: UpgradeCmd) -> i32 {
    match cmd {
        UpgradeCmd::Plan(args) => crate::commands::upgrade::plan::run(ctx, args).await,
    }
}

pub async fn dispatch_catalog(ctx: Ctx, cmd: CatalogCmd) -> i32 {
    match cmd {
        CatalogCmd::Plan(args) => crate::commands::catalog::plan::run(ctx, args).await,
    }
}

pub async fn dispatch_remediate(ctx: Ctx, cmd: RemediateCmd) -> i32 {
    match cmd {
        RemediateCmd::Draft(args) => crate::commands::remediate::draft::run(ctx, args).await,
    }
}

pub async fn dispatch_app(ctx: Ctx, cmd: AppCmd) -> i32 {
    match cmd {
        AppCmd::Adopt(args) => crate::commands::inventory::apps::adopt(ctx, args).await,
    }
}

pub async fn dispatch_mapping(ctx: Ctx, cmd: MappingCmd) -> i32 {
    match cmd {
        MappingCmd::Set(args) => crate::commands::environments::mappings::run_set(ctx, args).await,
        MappingCmd::Adopt(args) => {
            crate::commands::environments::mappings::run_adopt(ctx, args).await
        }
    }
}
