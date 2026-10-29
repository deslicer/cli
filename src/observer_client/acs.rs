//! Splunk Cloud ACS (Admin Config Service) Observer routes.

use serde::{Deserialize, Serialize};

use super::types::{ChangePlan, ChangePlanResponse};
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

    /// Create a Pending Cloud ACS plan: `POST /api/v1/plans` (stack_kind=cloud),
    /// then `POST /api/v1/acs/stacks/{stack}/project-plan` with `plan_id` so
    /// Observer attaches the projected items. Never approves or executes.
    pub async fn create_acs_project_plan(
        &self,
        stack: &str,
        name: Option<&str>,
        request: &AcsProjectPlanRequest,
    ) -> Result<AcsProjectPlanOutcome, CliError> {
        #[derive(Serialize)]
        struct CreateBody<'a> {
            source_type: &'a str,
            stack_kind: &'a str,
            acs_stack_name: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
        }
        let encoded = urlencoding_path_segment(stack)?;
        let created: ChangePlanResponse = self
            .request_json(
                reqwest::Method::POST,
                "api/v1/plans",
                Some(&CreateBody {
                    source_type: "manual",
                    stack_kind: "cloud",
                    acs_stack_name: stack,
                    name,
                }),
            )
            .await?;
        let plan = match created.plan {
            Some(plan) if created.success || !plan.id.is_empty() => plan,
            _ => {
                return Err(CliError::Other(format!(
                    "Cloud ACS plan creation failed: {}",
                    created.message
                )))
            }
        };

        #[derive(Serialize)]
        struct ProjectBody<'a> {
            #[serde(flatten)]
            request: &'a AcsProjectPlanRequest,
            plan_id: &'a str,
        }
        let path = format!("api/v1/acs/stacks/{encoded}/project-plan");
        let projected: AcsProjectPlanResponse = self
            .request_json(
                reqwest::Method::POST,
                &path,
                Some(&ProjectBody {
                    request,
                    plan_id: &plan.id,
                }),
            )
            .await?;
        Ok(AcsProjectPlanOutcome { plan, projected })
    }
}

/// One app in the project-plan body (conf tree keyed by relative path).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AcsProjectPlanApp {
    pub app_name: String,
    pub source_path: String,
    pub inventory_group: String,
    pub files: std::collections::BTreeMap<String, String>,
}

/// Body for `POST /api/v1/acs/stacks/{stack}/project-plan` (minus `plan_id`).
#[derive(Debug, Clone, Serialize)]
pub struct AcsProjectPlanRequest {
    pub environment_name: String,
    pub mapping_source: String,
    pub apps: Vec<AcsProjectPlanApp>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AcsProjectPlanItem {
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AcsProjectPlanResponse {
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub plan_status: Option<String>,
    #[serde(default)]
    pub item_count: usize,
    #[serde(default)]
    pub items: Vec<AcsProjectPlanItem>,
}

#[derive(Debug, Clone)]
pub struct AcsProjectPlanOutcome {
    /// Plan row returned by `POST /api/v1/plans`.
    pub plan: ChangePlan,
    pub projected: AcsProjectPlanResponse,
}

impl AcsProjectPlanOutcome {
    /// Plan with the status Observer reported after attaching items.
    pub fn as_change_plan(&self) -> ChangePlan {
        let mut plan = self.plan.clone();
        if let Some(status) = self.projected.plan_status.clone() {
            plan.status = status;
        }
        if plan.plan_id.is_none() {
            plan.plan_id = self.projected.plan_id.clone();
        }
        plan
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
