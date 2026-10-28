//! DAI routines — scheduled/agentic workflows on deslicer-ai (device session).

use clap::Subcommand;

use crate::Ctx;

mod client;
mod disable;
mod enable;
mod list;
mod report;
mod run;
mod runs;

#[derive(Subcommand)]
pub enum RoutineCmd {
    /// List DAI routines for this tenant
    List(list::Args),
    /// Enable a routine (dry-run unless --yes)
    Enable(enable::Args),
    /// Disable a routine (dry-run unless --yes)
    Disable(disable::Args),
    /// Trigger a routine run (dry-run unless --yes); optional --model override
    Run(run::Args),
    /// List recent runs for a routine
    Runs(runs::Args),
    /// Show a routine run report
    Report(report::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: RoutineCmd) -> i32 {
    match cmd {
        RoutineCmd::List(args) => list::run(ctx, args).await,
        RoutineCmd::Enable(args) => enable::run(ctx, args).await,
        RoutineCmd::Disable(args) => disable::run(ctx, args).await,
        RoutineCmd::Run(args) => run::run(ctx, args).await,
        RoutineCmd::Runs(args) => runs::run(ctx, args).await,
        RoutineCmd::Report(args) => report::run(ctx, args).await,
    }
}
