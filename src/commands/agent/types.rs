//! Wire types for the deslicer-ai CLI agent endpoints.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub visibility: String,
    #[serde(default, rename = "isOrchestrator")]
    pub is_orchestrator: bool,
}

/// A started run: response headers plus the still-open body.
pub struct StartedRun {
    pub run_id: Option<String>,
    pub conversation_id: Option<String>,
    pub response: reqwest::Response,
}

/// Where a run got to, as recorded in the server's ledger.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RunStatus {
    #[serde(rename = "runId")]
    pub run_id: String,
    pub status: String,
    #[serde(default, rename = "conversationId")]
    pub conversation_id: Option<String>,
    #[serde(default, rename = "errorCode")]
    pub error_code: Option<String>,
}

impl RunStatus {
    /// Whether the run has stopped moving, either way.
    pub fn is_terminal(&self) -> bool {
        self.status != "running"
    }
}

/// A run's status plus whatever answer has been persisted for it.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RunOutput {
    #[serde(flatten)]
    pub status: RunStatus,
    #[serde(default)]
    pub output: Option<String>,
}

/// One row from `GET /api/cli/agents/runs`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunListItem {
    #[serde(rename = "runId")]
    pub run_id: String,
    pub status: String,
    #[serde(default, rename = "agentId")]
    pub agent_id: Option<String>,
    #[serde(default, rename = "conversationId")]
    pub conversation_id: Option<String>,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    #[serde(default, rename = "finishedAt")]
    pub finished_at: Option<String>,
    #[serde(default, rename = "promptPreview")]
    pub prompt_preview: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RunListBody {
    #[serde(default)]
    pub runs: Vec<RunListItem>,
    #[serde(default, rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_status_reads_terminality_off_the_status_field() {
        let running: RunStatus =
            serde_json::from_str(r#"{"runId":"r","status":"running"}"#).expect("parse");
        assert!(!running.is_terminal());

        let done: RunStatus =
            serde_json::from_str(r#"{"runId":"r","status":"failed","errorCode":"abandoned"}"#)
                .expect("parse");
        assert!(done.is_terminal());
        assert_eq!(done.error_code.as_deref(), Some("abandoned"));
    }

    #[test]
    fn run_output_flattens_the_status_alongside_the_answer() {
        let parsed: RunOutput = serde_json::from_str(
            r#"{"runId":"r","status":"succeeded","conversationId":"c","output":"done"}"#,
        )
        .expect("parse");
        assert_eq!(parsed.status.status, "succeeded");
        assert_eq!(parsed.status.conversation_id.as_deref(), Some("c"));
        assert_eq!(parsed.output.as_deref(), Some("done"));
    }

    #[test]
    fn run_output_tolerates_an_answer_that_is_not_written_yet() {
        let parsed: RunOutput =
            serde_json::from_str(r#"{"runId":"r","status":"running"}"#).expect("parse");
        assert!(parsed.output.is_none());
    }

    #[test]
    fn agent_summary_defaults_is_orchestrator() {
        let parsed: AgentSummary =
            serde_json::from_str(r#"{"id":"a","name":"Slicer","visibility":"private"}"#)
                .expect("parse");
        assert!(!parsed.is_orchestrator);
    }
}
