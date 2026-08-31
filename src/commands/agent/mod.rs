use clap::Subcommand;

use crate::Ctx;

mod argv;
mod client;
mod http_errors;
mod ids;
mod list;
mod logs;
mod ls;
mod render;
mod repl;
mod resolve;
mod resume;
mod run;
mod session;
mod stream;
mod types;

pub use argv::rewrite_agent_argv;

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
    /// Continue this session's last conversation
    Resume(resume::Args),
    /// Start a conversation. `deslicer agent` with no subcommand does this.
    Repl(repl::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: AgentCmd) -> i32 {
    match cmd {
        AgentCmd::List(args) => list::run(ctx, args).await,
        AgentCmd::Ls(args) => ls::run(ctx, args).await,
        AgentCmd::Run(args) => run::run(ctx, args).await,
        AgentCmd::Logs(args) => logs::run(ctx, args).await,
        AgentCmd::Resume(args) => resume::run(ctx, args).await,
        AgentCmd::Repl(args) => repl::run(ctx, args).await,
    }
}

#[cfg(test)]
mod clap_tests {
    use clap::Parser;

    use crate::cli::{Cli, Command};

    use super::{rewrite_agent_argv, AgentCmd};

    fn parse_agent(args: &[&str]) -> AgentCmd {
        let rewritten = rewrite_agent_argv(args.iter().copied());
        match Cli::parse_from(rewritten).command {
            Command::Agent(cmd) => cmd,
            _ => panic!("expected an agent command"),
        }
    }

    #[test]
    fn list_ls_repl_and_resume_stay_distinct() {
        assert!(matches!(
            parse_agent(&["deslicer", "agent", "list"]),
            AgentCmd::List(_)
        ));
        assert!(matches!(
            parse_agent(&["deslicer", "agent", "ls"]),
            AgentCmd::Ls(_)
        ));
        assert!(matches!(
            parse_agent(&["deslicer", "agent", "repl"]),
            AgentCmd::Repl(_)
        ));
        assert!(matches!(
            parse_agent(&["deslicer", "agent", "resume"]),
            AgentCmd::Resume(_)
        ));
        assert!(matches!(
            parse_agent(&["deslicer", "agent"]),
            AgentCmd::Repl(_)
        ));
        assert!(matches!(
            parse_agent(&["deslicer", "agent", "hello"]),
            AgentCmd::Run(_)
        ));
    }
}
