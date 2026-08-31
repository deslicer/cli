//! Turns `--agent` into the UUID `POST /api/cli/agents/runs` accepts.

use crate::errors::CliError;

use super::client::AgentClient;
use super::types::AgentSummary;

/// Resolves a name or id to an agent UUID.
///
/// A UUID is returned as-is so a copied id never needs a list round trip.
/// Anything else is matched against `deslicer agent list`, case-insensitively.
/// The bare word `orchestrator` also matches the unique flagged default when
/// no agent is actually named that.
pub async fn resolve_agent(client: &AgentClient, raw: &str) -> Result<String, CliError> {
    let needle = raw.trim();
    if needle.is_empty() {
        return Err(CliError::Other(
            "an agent name or id is required. Run `deslicer agent list` to see \
             the agents you can run."
                .into(),
        ));
    }
    if uuid::Uuid::parse_str(needle).is_ok() {
        return Ok(needle.to_string());
    }
    let agents = client.list_agents().await?;
    resolve_from_list(&agents, needle)
}

pub fn resolve_from_list(agents: &[AgentSummary], raw: &str) -> Result<String, CliError> {
    let needle = raw.trim();
    let named: Vec<&AgentSummary> = agents
        .iter()
        .filter(|agent| agent.name.eq_ignore_ascii_case(needle))
        .collect();
    match named.as_slice() {
        [one] => return Ok(one.id.clone()),
        [] => {}
        many => return Err(ambiguous(needle, many)),
    }

    if needle.eq_ignore_ascii_case("orchestrator") {
        let flagged: Vec<&AgentSummary> = agents
            .iter()
            .filter(|agent| agent.is_orchestrator)
            .collect();
        match flagged.as_slice() {
            [one] => return Ok(one.id.clone()),
            [] => {}
            many => return Err(ambiguous("orchestrator", many)),
        }
    }

    Err(CliError::Other(format!(
        "no agent named '{needle}'. Run `deslicer agent list` to see the agents you can run."
    )))
}

fn ambiguous(needle: &str, matches: &[&AgentSummary]) -> CliError {
    let ids: Vec<&str> = matches.iter().map(|agent| agent.id.as_str()).collect();
    CliError::Other(format!(
        "'{needle}' matches more than one agent ({}). Pass an id from `deslicer agent list`.",
        ids.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, name: &str, is_orchestrator: bool) -> AgentSummary {
        AgentSummary {
            id: id.into(),
            name: name.into(),
            description: None,
            model: None,
            visibility: "private".into(),
            is_orchestrator,
        }
    }

    #[test]
    fn a_unique_name_resolves_case_insensitively() {
        let agents = [agent("a1", "Slicer", false), agent("a2", "Fleet", false)];
        assert_eq!(resolve_from_list(&agents, "slicer").expect("id"), "a1");
    }

    #[test]
    fn orchestrator_falls_back_to_the_flagged_default() {
        let agents = [
            agent("a1", "Slicer", false),
            agent("orch-1", "Tenant Orchestrator", true),
        ];
        assert_eq!(
            resolve_from_list(&agents, "orchestrator").expect("id"),
            "orch-1"
        );
    }

    #[test]
    fn a_real_name_wins_over_the_orchestrator_alias() {
        let agents = [
            agent("named", "Orchestrator", false),
            agent("flagged", "Tenant Orchestrator", true),
        ];
        assert_eq!(
            resolve_from_list(&agents, "orchestrator").expect("id"),
            "named"
        );
    }

    #[test]
    fn two_agents_with_the_same_name_are_ambiguous() {
        let agents = [agent("a1", "Slicer", false), agent("a2", "Slicer", false)];
        let err = resolve_from_list(&agents, "Slicer").expect_err("ambiguous");
        assert!(err.to_string().contains("more than one"), "{err}");
        assert!(err.to_string().contains("a1"), "{err}");
    }

    #[test]
    fn an_unknown_name_points_at_the_list_command() {
        let err = resolve_from_list(&[agent("a1", "Slicer", false)], "nope").expect_err("missing");
        assert!(err.to_string().contains("deslicer agent list"), "{err}");
    }

    #[test]
    fn two_flagged_orchestrators_are_ambiguous() {
        let agents = [agent("a1", "One", true), agent("a2", "Two", true)];
        let err = resolve_from_list(&agents, "orchestrator").expect_err("ambiguous");
        assert!(err.to_string().contains("more than one"), "{err}");
    }
}
