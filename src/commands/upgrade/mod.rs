use clap::Subcommand;

use crate::Ctx;

mod plan;
mod status;

#[derive(Subcommand)]
pub enum UpgradeCmd {
    /// Show Splunk upgrade / flapping-gate status
    Status(status::Args),
    /// Create a Pending upgrade plan (dry-run unless --yes)
    Plan(plan::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: UpgradeCmd) -> i32 {
    match cmd {
        UpgradeCmd::Status(args) => status::run(ctx, args).await,
        UpgradeCmd::Plan(args) => plan::run(ctx, args).await,
    }
}
