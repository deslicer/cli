use clap::Subcommand;

use crate::Ctx;

mod bootstrap;
mod refresh;
mod session;
mod status;

#[derive(Subcommand)]
pub enum RepoCmd {
    /// Create a private org repo via the GitHub App (dry-run unless --yes)
    Bootstrap(bootstrap::Args),
    /// Open a workflow refresh pull request for one repo
    Refresh(refresh::Args),
    /// List linked repos and bootstrap job fields
    Status(status::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: RepoCmd) -> i32 {
    match cmd {
        RepoCmd::Bootstrap(args) => bootstrap::run(ctx, args).await,
        RepoCmd::Refresh(args) => refresh::run(ctx, args).await,
        RepoCmd::Status(args) => status::run(ctx, args).await,
    }
}
