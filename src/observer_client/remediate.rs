//! Remediate analyse / draft (draft plan id).

use serde::{Deserialize, Serialize};

use super::types::{ChangePlan, ChangePlanResponse};
use super::Client;
use crate::errors::CliError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediateAnalysis {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub findings: Option<serde_json::Value>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediateDraft {
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub plan: Option<ChangePlan>,
    #[serde(default)]
    pub message: Option<String>,
}

impl Client {
    pub async fn remediate_analyse(
        &self,
        host: Option<&str>,
        app: Option<&str>,
        group: Option<&str>,
    ) -> Result<RemediateAnalysis, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            host: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            app: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            inventory_group: Option<&'a str>,
        }
        self.request_json(
            reqwest::Method::POST,
            "api/v1/remediate/analyse",
            Some(&Body {
                host,
                app,
                inventory_group: group,
            }),
        )
        .await
    }

    /// Create a draft remediation plan; returns draft plan id (Pending).
    pub async fn remediate_draft(
        &self,
        host: Option<&str>,
        app: Option<&str>,
        group: Option<&str>,
        name: Option<&str>,
    ) -> Result<RemediateDraft, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            host: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            app: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            inventory_group: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
        }
        let bytes = self
            .request_bytes(
                reqwest::Method::POST,
                "api/v1/remediate/draft",
                Some(&Body {
                    host,
                    app,
                    inventory_group: group,
                    name,
                }),
            )
            .await?;
        if let Ok(draft) = serde_json::from_slice::<RemediateDraft>(&bytes) {
            return Ok(draft);
        }
        if let Ok(envelope) = serde_json::from_slice::<ChangePlanResponse>(&bytes) {
            if let Some(plan) = envelope.plan {
                return Ok(RemediateDraft {
                    plan_id: Some(plan.external_id().to_string()),
                    plan: Some(plan),
                    message: if envelope.message.is_empty() {
                        None
                    } else {
                        Some(envelope.message)
                    },
                });
            }
        }
        if let Ok(plan) = serde_json::from_slice::<ChangePlan>(&bytes) {
            return Ok(RemediateDraft {
                plan_id: Some(plan.external_id().to_string()),
                plan: Some(plan),
                message: None,
            });
        }
        Err(CliError::Transport(
            "invalid remediate draft response".into(),
        ))
    }
}
