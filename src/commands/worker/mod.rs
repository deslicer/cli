use clap::Subcommand;

use crate::Ctx;

mod instructions;
mod snippet;

#[derive(Subcommand)]
pub enum WorkerCmd {
    /// Print a worker install recipe from the portal (no SSH)
    Instructions(instructions::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: WorkerCmd) -> i32 {
    match cmd {
        WorkerCmd::Instructions(args) => instructions::run(ctx, args).await,
    }
}
