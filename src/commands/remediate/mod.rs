use clap::Subcommand;

use crate::Ctx;

mod analyse;
mod draft;

#[derive(Subcommand)]
pub enum RemediateCmd {
    /// Analyse remediation candidates (read-only)
    Analyse(analyse::Args),
    /// Create a draft remediation plan id (dry-run unless --yes)
    Draft(draft::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: RemediateCmd) -> i32 {
    match cmd {
        RemediateCmd::Analyse(args) => analyse::run(ctx, args).await,
        RemediateCmd::Draft(args) => draft::run(ctx, args).await,
    }
}
