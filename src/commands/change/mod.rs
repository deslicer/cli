use clap::Subcommand;

use crate::Ctx;

pub mod approve;
pub mod deploy;
pub mod plan;
mod plan_env;
pub mod reject;
pub mod show;
pub mod status;
pub mod verify;

#[derive(Subcommand)]
pub enum ChangeCmd {
    Plan(plan::Args),
    Show(show::Args),
    Approve(approve::Args),
    Reject(reject::Args),
    Deploy(deploy::Args),
    Verify(verify::Args),
    Status(status::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: ChangeCmd) -> i32 {
    match cmd {
        ChangeCmd::Plan(args) => plan::run(ctx, args).await,
        ChangeCmd::Show(args) => show::run(ctx, args).await,
        ChangeCmd::Approve(args) => approve::run(ctx, args).await,
        ChangeCmd::Reject(args) => reject::run(ctx, args).await,
        ChangeCmd::Deploy(args) => deploy::run(ctx, args).await,
        ChangeCmd::Verify(args) => verify::run(ctx, args).await,
        ChangeCmd::Status(args) => status::run(ctx, args).await,
    }
}
