//! Tenant model catalogue and BYOK (device session → deslicer-ai).

use clap::Subcommand;

use crate::Ctx;

mod byok;
mod client;
mod list;

#[derive(Subcommand)]
pub enum ModelsCmd {
    /// List available models for this tenant
    List(list::Args),
    /// Bring-your-own-key status / set
    #[command(subcommand)]
    Byok(byok::ByokCmd),
}

pub async fn dispatch(ctx: Ctx, cmd: ModelsCmd) -> i32 {
    match cmd {
        ModelsCmd::List(args) => list::run(ctx, args).await,
        ModelsCmd::Byok(cmd) => byok::dispatch(ctx, cmd).await,
    }
}
