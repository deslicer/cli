use clap::Subcommand;

use crate::Ctx;

mod bind;
mod status;

#[derive(Subcommand)]
pub enum LicenseCmd {
    /// Show tenant license status
    Status(status::Args),
    /// Bind a license key (dry-run unless --yes)
    Bind(bind::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: LicenseCmd) -> i32 {
    match cmd {
        LicenseCmd::Status(args) => status::run(ctx, args).await,
        LicenseCmd::Bind(args) => bind::run(ctx, args).await,
    }
}
