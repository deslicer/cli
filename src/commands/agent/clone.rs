use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::errors::CliError;
use crate::Ctx;

use super::client::{AgentClient, AgentSummary};

#[derive(ClapArgs)]
pub struct Args {
    /// Agent id to clone (from `agent list`)
    pub id: String,

    /// Additional agent ids to clone in the same request
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub bulk: Vec<String>,

    /// Perform the clone(s). Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let log_format = ctx.log_format;
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(log_format, err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    let mut ids = vec![args.id.clone()];
    ids.extend(args.bulk.iter().cloned());
    ids.sort();
    ids.dedup();

    if !args.yes {
        print_dry_run(&ctx, &ids);
        return Ok(0);
    }

    let client = AgentClient::from_ctx(&ctx)?;
    let cloned = client.clone_agents(&ids).await?;
    match ctx.log_format {
        LogFormat::Json => {
            let payload = json!({ "agents": cloned, "source_ids": ids });
            println!(
                "{}",
                serde_json::to_string_pretty(&payload)
                    .map_err(|e| CliError::Other(format!("serialize: {e}")))?
            );
        }
        LogFormat::Human => print!("{}", format_cloned_human(&cloned)),
    }
    Ok(0)
}

fn print_dry_run(ctx: &Ctx, ids: &[String]) {
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            json!({ "dry_run": true, "action": "clone", "ids": ids })
        ),
        LogFormat::Human => {
            println!(
                "dry-run: would clone agent(s): {}\npass --yes to create copies",
                ids.join(", ")
            );
        }
    }
}

fn format_cloned_human(agents: &[AgentSummary]) -> String {
    if agents.is_empty() {
        return "No agents were cloned.\n".to_string();
    }
    let mut lines = vec!["cloned:".to_string()];
    for agent in agents {
        lines.push(format!("  {}  {}", agent.id, agent.name));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn help_mentions_bulk_and_yes() {
        let mut cmd = crate::cli::Cli::command();
        let agent = cmd.find_subcommand_mut("agent").expect("agent");
        let clone = agent.find_subcommand_mut("clone").expect("clone");
        let mut buf = Vec::new();
        clone.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--yes"));
        assert!(help.contains("--bulk"));
    }

    #[test]
    fn dry_run_lists_ids() {
        let ids: Vec<String> = vec!["a".into(), "b".into()];
        let text = format!(
            "dry-run: would clone agent(s): {}\npass --yes to create copies",
            ids.join(", ")
        );
        assert!(text.contains("a, b"));
        assert!(text.contains("--yes"));
    }
}
