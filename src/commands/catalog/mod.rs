use clap::Subcommand;

use crate::Ctx;

mod list;
mod plan;
mod show;

#[derive(Subcommand)]
pub enum CatalogCmd {
    /// List catalog blueprints
    List(list::Args),
    /// Show one catalog blueprint
    Show(show::Args),
    /// Create Pending plan(s) from a blueprint for --targets (dry-run unless --yes)
    Plan(plan::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: CatalogCmd) -> i32 {
    match cmd {
        CatalogCmd::List(args) => list::run(ctx, args).await,
        CatalogCmd::Show(args) => show::run(ctx, args).await,
        CatalogCmd::Plan(args) => plan::run(ctx, args).await,
    }
}
