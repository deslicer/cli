//! Splunk Cloud stack commands.
//!
//! Canonical path: `cloud stack …`. `acs` and the plural nouns `stacks` /
//! `credentials` remain hidden aliases (#187).

use clap::Subcommand;

use crate::Ctx;

pub mod credentials;
pub mod observe;
pub mod stacks;

#[derive(Subcommand)]
pub enum CloudCmd {
    /// List, credential, or observe one Splunk Cloud stack.
    #[command(name = "stack", alias = "stacks", subcommand)]
    Stack(StackCmd),
    /// Deprecated location of `cloud stack credential`.
    #[command(name = "credential", alias = "credentials", hide = true, subcommand)]
    Credential(credentials::CredentialsCmd),
    /// Deprecated location of `cloud stack observe`.
    #[command(hide = true)]
    Observe(observe::Args),
}

#[derive(Subcommand)]
pub enum StackCmd {
    /// List Splunk Cloud stacks that have a stored credential.
    List(stacks::ListArgs),
    /// List, test, or set the credential for one stack.
    #[command(name = "credential", alias = "credentials", subcommand)]
    Credential(credentials::CredentialsCmd),
    /// Observe a stack (read-only scan; never applies).
    Observe(observe::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: CloudCmd) -> i32 {
    match cmd {
        CloudCmd::Stack(cmd) => dispatch_stack(ctx, cmd).await,
        CloudCmd::Credential(cmd) => credentials::dispatch(ctx, cmd).await,
        CloudCmd::Observe(args) => observe::run(ctx, args).await,
    }
}

async fn dispatch_stack(ctx: Ctx, cmd: StackCmd) -> i32 {
    match cmd {
        StackCmd::List(args) => stacks::list(ctx, args).await,
        StackCmd::Credential(cmd) => credentials::dispatch(ctx, cmd).await,
        StackCmd::Observe(args) => observe::run(ctx, args).await,
    }
}
