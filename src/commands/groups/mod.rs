use clap::Subcommand;

use crate::Ctx;

pub mod list;

#[derive(Subcommand)]
pub enum GroupsCmd {
    /// List host groups (`id` or exact `name` for `change plan --target-group`)
    List(list::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: GroupsCmd) -> i32 {
    match cmd {
        GroupsCmd::List(args) => list::run(ctx, args).await,
    }
}
