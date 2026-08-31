use clap::Subcommand;

use crate::Ctx;

mod client;
mod http_errors;
mod ids;
mod list;
mod logs;
mod ls;
mod render;
mod resolve;
mod run;
mod stream;
mod types;

#[derive(Subcommand)]
pub enum AgentCmd {
    /// List the agents this session can run
    List(list::Args),
    /// List recent runs started by this session
    Ls(ls::Args),
    /// Run an agent and stream its answer
    Run(run::Args),
    /// Read the output of a run that is already going
    Logs(logs::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: AgentCmd) -> i32 {
    match cmd {
        AgentCmd::List(args) => list::run(ctx, args).await,
        AgentCmd::Ls(args) => ls::run(ctx, args).await,
        AgentCmd::Run(args) => run::run(ctx, args).await,
        AgentCmd::Logs(args) => logs::run(ctx, args).await,
    }
}
