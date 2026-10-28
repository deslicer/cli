//! Skills marketplace on deslicer-ai (device session).

use clap::Subcommand;

use crate::Ctx;

mod client;
mod import;
mod list;

#[derive(Subcommand)]
pub enum SkillCmd {
    /// List installed / marketplace skills (size, rename, scan results)
    List(list::Args),
    /// Import a skill from a path or marketplace id (dry-run unless --yes)
    Import(import::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: SkillCmd) -> i32 {
    match cmd {
        SkillCmd::List(args) => list::run(ctx, args).await,
        SkillCmd::Import(args) => import::run(ctx, args).await,
    }
}
