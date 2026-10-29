use clap::Subcommand;

use crate::Ctx;

pub mod credentials;
pub mod observe;
pub mod stacks;

#[derive(Subcommand)]
pub enum AcsCmd {
    /// List Splunk Cloud ACS stacks for the tenant
    #[command(subcommand)]
    Stacks(stacks::StacksCmd),
    /// List, test or set Splunk Cloud ACS stack credentials
    #[command(subcommand)]
    Credentials(credentials::CredentialsCmd),
    /// Observe an ACS stack (read-only scan; never applies)
    Observe(observe::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: AcsCmd) -> i32 {
    match cmd {
        AcsCmd::Stacks(cmd) => stacks::dispatch(ctx, cmd).await,
        AcsCmd::Credentials(cmd) => credentials::dispatch(ctx, cmd).await,
        AcsCmd::Observe(args) => observe::run(ctx, args).await,
    }
}
