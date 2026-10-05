use clap::Subcommand;

use crate::Ctx;

pub mod format;
pub mod login;
pub mod logout;
pub mod status;
pub mod whoami;

#[derive(Subcommand)]
pub enum AuthCmd {
    /// Exchange CI OIDC or start device login and store credentials for later commands.
    Login(login::Args),
    /// Clear the stored device session (environment tokens are unchanged).
    Logout(logout::Args),
    /// Print CI platform, OIDC availability, and environment binding diagnostics.
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
