use clap::Subcommand;

use crate::Ctx;

mod create;
mod jti;
mod list;
mod revoke;
mod write_token;

#[derive(Subcommand)]
pub enum EnrollCmd {
    /// Mint a one-time enrollment token (shown once)
    Create(create::Args),
    /// List enrollment tokens (never reprints the secret)
    List(list::Args),
    /// Revoke a token by UUID jti
    Revoke(revoke::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: EnrollCmd) -> i32 {
    match cmd {
        EnrollCmd::Create(args) => create::run(ctx, args).await,
        EnrollCmd::List(args) => list::run(ctx, args).await,
        EnrollCmd::Revoke(args) => revoke::run(ctx, args).await,
    }
}
