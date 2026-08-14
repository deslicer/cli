use clap::Subcommand;

use crate::Ctx;

pub mod login;
pub mod logout;
pub mod status;

#[derive(Subcommand)]
pub enum AuthCmd {
    Login(login::Args),
    Logout(logout::Args),
    Status(status::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: AuthCmd) -> i32 {
    match cmd {
        AuthCmd::Login(args) => login::run(ctx, args).await,
        AuthCmd::Logout(args) => logout::run(ctx, args).await,
        AuthCmd::Status(args) => status::run(ctx, args).await,
    }
}
