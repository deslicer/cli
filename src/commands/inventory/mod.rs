use clap::Subcommand;

use crate::Ctx;

pub mod list;

#[derive(Subcommand)]
pub enum InventoryCmd {
    /// List Ansible inventory groups and their hosts
    List(list::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: InventoryCmd) -> i32 {
    match cmd {
        InventoryCmd::List(args) => list::run(ctx, args).await,
    }
}
