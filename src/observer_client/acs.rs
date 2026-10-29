//! Splunk Cloud ACS (Admin Config Service) Observer routes.

use serde::{Deserialize, Serialize};

use super::types::ChangePlan;
use super::Client;
use crate::errors::CliError;

/// One ACS stack known to the tenant. Derived from the stack credential list
/// (`GET /api/v1/acs/stacks/credentials`); Observer has no separate stack
/// catalogue route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcsStack {
    pub name: String,
    /// ACS environment for the stack (`production` or `staging`).
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub token_configured: bool,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// ACS stack credential metadata. The JWT itself is write-only on Observer and
/// is never returned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcsCredential {
    #[serde(default)]
    pub id: Option<String>,
    pub stack_name: String,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub token_configured: bool,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// Back-compat alias for older callers.
pub type SplunkCredential = AcsCredential;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialTestResult {
    pub ok: bool,
    pub stack: String,
    #[serde(default)]
    pub object_count: Option<usize>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcsObservedObject {
    pub resource_type: String,
    pub resource_id: String,
    #[serde(default)]
    pub content_hash: Option<String>,
}

/// Response of `POST /api/v1/acs/stacks/{stack}/observe` (read-only scan).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcsObserveReport {
    #[serde(alias = "stack_name")]
    pub stack: String,
    #[serde(default)]
    pub object_count: usize,
    #[serde(default)]
    pub objects: Vec<AcsObservedObject>,
}

#[derive(Deserialize)]
struct ListCredentialsResponse {
    #[serde(default)]
    credentials: Vec<AcsCredential>,
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

/// ACS environments accepted by Observer for stack credentials.
pub fn validate_acs_environment(raw: &str) -> Result<&str, CliError> {
    match raw {
        "production" | "staging" => Ok(raw),
        other => Err(CliError::InvalidInput(format!(
            "--acs-environment must be production or staging (got {other:?})"
        ))),
    }
}

impl Client {
    /// Stacks with a stored ACS credential (`GET /api/v1/acs/stacks/credentials`).
    pub async fn list_acs_stacks(&self) -> Result<Vec<AcsStack>, CliError> {
        let creds = self.list_acs_credentials().await?;
        Ok(stacks_from_credentials(creds))
    }

    /// ACS stack credential metadata (`GET /api/v1/acs/stacks/credentials`).
    pub async fn list_acs_credentials(&self) -> Result<Vec<AcsCredential>, CliError> {
        let resp: ListCredentialsResponse = self.get_json("api/v1/acs/stacks/credentials").await?;
        Ok(resp.credentials)
    }

    /// Back-compat name for [`Client::list_acs_credentials`].
    pub async fn list_splunk_credentials(&self) -> Result<Vec<AcsCredential>, CliError> {
        self.list_acs_credentials().await
    }

    /// Store or rotate the ACS JWT for a stack
    /// (`PUT /api/v1/acs/stacks/{stack}/credentials`). The token is sent once
    /// and never echoed back.
    pub async fn set_acs_credential(
        &self,
        stack: &str,
        token: &str,
        acs_environment: Option<&str>,
    ) -> Result<AcsCredential, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            token: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            environment: Option<&'a str>,
        }
        let encoded = urlencoding_path_segment(stack)?;
        let path = format!("api/v1/acs/stacks/{encoded}/credentials");
        self.request_json(
            reqwest::Method::PUT,
            &path,
            Some(&Body {
                token,
                environment: acs_environment,
            }),
        )
        .await
    }

    /// Check that the stored credential for `stack` can reach ACS by running a
    /// read-only observe scan.
    pub async fn test_splunk_credential(
        &self,
        stack: &str,
    ) -> Result<CredentialTestResult, CliError> {
        match self.observe_acs_stack(stack, None).await {
            Ok(report) => Ok(CredentialTestResult {
                ok: true,
                stack: stack.to_string(),
                object_count: Some(report.object_count),
                message: None,
            }),
            // 409 (no credential) and 502 (ACS rejected the JWT / stack not
            // found) surface as Other / BackendUnavailable: report as a failed
            // test instead of a transport error.
            Err(CliError::Other(message))
            | Err(CliError::AmbiguousBinding(message))
            | Err(CliError::BackendUnavailable(message)) => Ok(CredentialTestResult {
                ok: false,
                stack: stack.to_string(),
                object_count: None,
                message: Some(message),
            }),
            Err(err) => Err(err),
        }
    }

    /// Read-only ACS observe scan (`POST /api/v1/acs/stacks/{stack}/observe`).
    pub async fn observe_acs_stack(
        &self,
        stack: &str,
        experience: Option<&str>,
    ) -> Result<AcsObserveReport, CliError> {
        let encoded = urlencoding_path_segment(stack)?;
        let path = match experience {
            Some(exp) => format!(
                "api/v1/acs/stacks/{encoded}/observe?experience={}",
                urlencoding_path_segment(exp)?
            ),
            None => format!("api/v1/acs/stacks/{encoded}/observe"),
        };
        self.request_json(reqwest::Method::POST, &path, None::<&()>)
            .await
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

fn stacks_from_credentials(creds: Vec<AcsCredential>) -> Vec<AcsStack> {
    let mut stacks: Vec<AcsStack> = creds
        .into_iter()
        .map(|c| AcsStack {
            name: c.stack_name,
            environment: c.environment,
            token_configured: c.token_configured,
            updated_at: c.updated_at,
        })
        .collect();
    stacks.sort_by(|a, b| a.name.cmp(&b.name));
    stacks.dedup_by(|a, b| a.name == b.name);
    stacks
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
