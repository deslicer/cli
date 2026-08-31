use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::map_cli_error;
use crate::errors::CliError;
use crate::Ctx;

use super::client::{AgentClient, AgentSummary};

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    match run_inner(ctx).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: Ctx) -> Result<i32, CliError> {
    let agents = AgentClient::from_ctx(&ctx)?.list_agents().await?;

    match ctx.log_format {
        LogFormat::Json => {
            let payload = serde_json::json!({ "agents": agents });
            let text = serde_json::to_string_pretty(&payload)
                .map_err(|err| CliError::Other(format!("serialize agents: {err}")))?;
            println!("{text}");
        }
        LogFormat::Human => print!("{}", format_agents_human(&agents)),
    }
    Ok(0)
}

fn format_agents_human(agents: &[AgentSummary]) -> String {
    if agents.is_empty() {
        return "No agents available. Create one in the portal first.\n".to_string();
    }

    let mut lines = vec!["ID  VISIBILITY  MODEL  NAME".to_string()];
    for agent in agents {
        let name = if agent.is_orchestrator {
            format!("{} (default)", agent.name)
        } else {
            agent.name.clone()
        };
        lines.push(format!(
            "{}  {}  {}  {}",
            agent.id,
            agent.visibility,
            agent.model.as_deref().unwrap_or("-"),
            name,
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, name: &str) -> AgentSummary {
        AgentSummary {
            id: id.into(),
            name: name.into(),
            description: None,
            model: Some("claude-sonnet-4".into()),
            visibility: "private".into(),
            is_orchestrator: false,
        }
    }

    #[test]
    fn marks_the_orchestrator_as_the_default() {
        let mut a = agent("a1", "Orchestrator");
        a.is_orchestrator = true;
        assert!(format_agents_human(&[a]).contains("Orchestrator (default)"));
    }

    #[test]
    fn lists_id_first_so_it_can_be_copied_into_agent_run() {
        let text = format_agents_human(&[agent("a1", "Slicer")]);
        assert!(text.starts_with("ID  VISIBILITY  MODEL  NAME\na1  private"));
    }

    #[test]
    fn missing_model_renders_as_a_dash() {
        let mut a = agent("a1", "Slicer");
        a.model = None;
        assert!(format_agents_human(&[a]).contains("a1  private  -  Slicer"));
    }

    #[test]
    fn empty_list_explains_what_to_do() {
        assert!(format_agents_human(&[]).contains("portal"));
    }
}
