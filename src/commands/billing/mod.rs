//! Billing usage / credits (read-only, device session → deslicer-ai).

use clap::Subcommand;

use crate::Ctx;

mod client;
mod credits;
mod usage;

#[derive(Subcommand)]
pub enum BillingCmd {
    /// Show usage for the current billing period
    Usage(usage::Args),
    /// Show credit balance
    Credits(credits::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: BillingCmd) -> i32 {
    match cmd {
        BillingCmd::Usage(args) => usage::run(ctx, args).await,
        BillingCmd::Credits(args) => credits::run(ctx, args).await,
    }
}
