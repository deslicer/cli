//! Splunk Cloud ACS (Admin Config Service) Observer routes.

use serde::{Deserialize, Serialize};

use super::types::ChangePlan;
use super::Client;
use crate::errors::CliError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcsStack {
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplunkCredential {
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialTestResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcsObserveReport {
    pub stack: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub project_plan: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ListStacksResponse {
    #[serde(default)]
    stacks: Vec<AcsStack>,
}

#[derive(Deserialize)]
struct ListCredentialsResponse {
    #[serde(default)]
    credentials: Vec<SplunkCredential>,
}

#[derive(Deserialize)]
struct AcsDraftResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: String,
    #[serde(default)]
    plan: Option<ChangePlan>,
}

impl Client {
    pub async fn list_acs_stacks(&self) -> Result<Vec<AcsStack>, CliError> {
        let resp: ListStacksResponse = self.get_json("api/v1/acs/stacks").await?;
        Ok(resp.stacks)
    }

    pub async fn list_splunk_credentials(&self) -> Result<Vec<SplunkCredential>, CliError> {
        let resp: ListCredentialsResponse =
            self.get_json("api/v1/tenant/splunk-credentials").await?;
        Ok(resp.credentials)
    }

    pub async fn test_splunk_credential(
        &self,
        name: &str,
    ) -> Result<CredentialTestResult, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            name: &'a str,
        }
        self.request_json(
            reqwest::Method::POST,
            "api/v1/tenant/splunk-credentials/test",
            Some(&Body { name }),
        )
        .await
    }

    /// Read-only ACS project plan / observe payload for a stack.
    pub async fn observe_acs_stack(&self, stack: &str) -> Result<AcsObserveReport, CliError> {
        let encoded = urlencoding_path_segment(stack)?;
        let path = format!("api/v1/acs/stacks/{encoded}/project-plan");
        let project_plan: serde_json::Value = self.get_json(&path).await?;
        Ok(AcsObserveReport {
            stack: stack.to_string(),
            summary: project_plan
                .get("summary")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            project_plan: Some(project_plan),
        })
    }

    /// Create a Pending ACS draft plan for a stack (never auto-approves).
    pub async fn create_acs_draft_plan(
        &self,
        stack: &str,
        name: Option<&str>,
    ) -> Result<ChangePlan, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
        }
        let encoded = urlencoding_path_segment(stack)?;
        let path = format!("api/v1/acs/stacks/{encoded}/project-plan");
        let bytes = self
            .request_bytes(reqwest::Method::POST, &path, Some(&Body { name }))
            .await?;
        if let Ok(envelope) = serde_json::from_slice::<AcsDraftResponse>(&bytes) {
            if let Some(plan) = envelope.plan {
                return Ok(plan);
            }
            if !envelope.success && !envelope.message.is_empty() {
                return Err(CliError::Other(envelope.message));
            }
        }
        // Some Observer builds return a bare plan or `{ plan_id }` then require
        // a follow-up acs-draft call; try bare plan first.
        if let Ok(plan) = serde_json::from_slice::<ChangePlan>(&bytes) {
            return Ok(plan);
        }
        #[derive(Deserialize)]
        struct IdOnly {
            plan_id: String,
        }
        if let Ok(id_only) = serde_json::from_slice::<IdOnly>(&bytes) {
            let draft_path = format!("api/v1/plans/{}/acs-draft", id_only.plan_id);
            let draft_bytes = self
                .request_bytes(reqwest::Method::POST, &draft_path, None::<&()>)
                .await?;
            if let Ok(envelope) = serde_json::from_slice::<AcsDraftResponse>(&draft_bytes) {
                if let Some(plan) = envelope.plan {
                    return Ok(plan);
                }
            }
            return serde_json::from_slice::<ChangePlan>(&draft_bytes)
                .map_err(|e| CliError::Transport(format!("invalid ACS draft plan JSON: {e}")));
        }
        Err(CliError::Transport(
            "invalid ACS project-plan create response".into(),
        ))
    }
}

fn urlencoding_path_segment(raw: &str) -> Result<String, CliError> {
    if raw.is_empty()
        || raw.contains('/')
        || raw.contains("..")
        || raw.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(CliError::InvalidInput(
            "ACS stack name must be a non-empty path segment".into(),
        ));
    }
    Ok(raw
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_stack_names() {
        assert!(urlencoding_path_segment("").is_err());
        assert!(urlencoding_path_segment("a/b").is_err());
        assert!(urlencoding_path_segment("a b").is_err());
        assert!(urlencoding_path_segment("prod-stack").is_ok());
    }
}
