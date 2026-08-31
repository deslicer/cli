use clap::Subcommand;

use crate::Ctx;

mod client;
mod list;
mod render;
mod run;
mod stream;

#[derive(Subcommand)]
pub enum AgentCmd {
    /// List the agents this session can run
    List(list::Args),
    /// Run an agent and stream its answer
    Run(run::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: AgentCmd) -> i32 {
    match cmd {
        AgentCmd::List(args) => list::run(ctx, args).await,
        AgentCmd::Run(args) => run::run(ctx, args).await,
    }
}
