//! Shape checks for the ids the server issues.
//!
//! Agent, conversation and run ids are all UUIDs server-side. Checking that
//! locally buys two things: a typo reads as a bad argument naming the command
//! that lists valid values, rather than an opaque `Invalid request: agentId.`
//! after a round trip; and the two ids that land in a URL path cannot reshape
//! the request into one for a different endpoint.

use crate::errors::CliError;

/// Validates an agent id, as accepted by `--agent`.
pub fn parse_agent_id(raw: &str) -> Result<&str, CliError> {
    require_uuid(
        raw,
        "an agent id",
        "Run `deslicer agent list` to see the agents you can run.",
    )
}

/// Validates a conversation id, as accepted by `--conversation`.
pub fn parse_conversation_id(raw: &str) -> Result<&str, CliError> {
    require_uuid(
        raw,
        "a conversation id",
        "Conversation ids are printed when a run starts.",
    )
}

/// Validates a run id, as accepted by `agent logs` and the run endpoints.
pub fn parse_run_id(raw: &str) -> Result<&str, CliError> {
    require_uuid(
        raw,
        "a run id",
        "Run ids are printed when a run starts, and are scoped to the account \
         that started them.",
    )
}

fn require_uuid<'a>(raw: &'a str, what: &str, remedy: &str) -> Result<&'a str, CliError> {
    if uuid::Uuid::parse_str(raw).is_err() {
        return Err(CliError::Other(format!("'{raw}' is not {what}. {remedy}")));
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "11111111-1111-4111-8111-111111111111";

    #[test]
    fn a_valid_uuid_passes_through_unchanged() {
        assert_eq!(parse_agent_id(VALID).expect("valid"), VALID);
    }

    #[test]
    fn an_agent_typo_names_the_list_command() {
        // The likeliest mistake is passing the agent's display name, so the
        // remedy has to say where the real ids come from.
        let err = parse_agent_id("slicer-agent").expect_err("should reject");
        assert!(err.to_string().contains("deslicer agent list"), "{err}");
    }

    #[test]
    fn a_path_traversal_segment_is_rejected_before_it_reaches_a_url() {
        let err = parse_run_id("../../admin").expect_err("should reject");
        assert!(err.to_string().contains("not a run id"), "{err}");
    }

    #[test]
    fn each_id_kind_explains_where_its_values_come_from() {
        // Same failure, three different things the caller should go look at.
        assert!(parse_conversation_id("nope")
            .expect_err("reject")
            .to_string()
            .contains("printed when a run starts"));
        assert!(parse_run_id("nope")
            .expect_err("reject")
            .to_string()
            .contains("scoped to the account"));
    }

    #[test]
    fn an_empty_id_is_rejected_rather_than_sent_as_a_blank_segment() {
        assert!(parse_run_id("").is_err());
    }
}
