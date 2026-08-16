use clap::Subcommand;

use crate::Ctx;

pub mod format;
pub mod login;
pub mod logout;
pub mod status;
pub mod whoami;

#[derive(Subcommand)]
pub enum AuthCmd {
    Login(login::Args),
    Logout(logout::Args),
    Status(status::Args),
    /// Print the current CLI identity without dumping tokens
    Whoami(whoami::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: AuthCmd) -> i32 {
    match cmd {
        AuthCmd::Login(args) => login::run(ctx, args).await,
        AuthCmd::Logout(args) => logout::run(ctx, args).await,
        AuthCmd::Status(args) => status::run(ctx, args).await,
        AuthCmd::Whoami(args) => whoami::run(ctx, args).await,
    }
}
