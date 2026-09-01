use clap::Subcommand;

use crate::Ctx;

pub mod list;
pub mod sync;

#[derive(Subcommand)]
pub enum InventoryCmd {
    /// List Ansible inventory groups and their hosts
    List(list::Args),
    /// Refresh `.deslicer/environments/<tenant-slug>.yml` from Observer host groups
    Sync(sync::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: InventoryCmd) -> i32 {
    match cmd {
        InventoryCmd::List(args) => list::run(ctx, args).await,
        InventoryCmd::Sync(args) => sync::run(ctx, args).await,
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn clap_exposes_inventory_sync() {
        let mut cmd = crate::cli::Cli::command();
        let inventory = cmd.find_subcommand_mut("inventory").expect("inventory");
        assert!(inventory.find_subcommand("sync").is_some());
        assert!(inventory.find_subcommand("list").is_some());
    }
}
