//! Splunk package upgrade intent (`/api/v1/splunk-packages/upgrade*`).

use serde::{Deserialize, Serialize};

use super::types::{ChangePlan, ChangePlanResponse};
use super::Client;
use crate::errors::CliError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeStatus {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub flapping_gate: Option<String>,
    #[serde(default)]
    pub flapping_gate_ok: Option<bool>,
    #[serde(default)]
    pub current_version: Option<String>,
    #[serde(default)]
    pub target_version: Option<String>,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub detail: Option<serde_json::Value>,
}

impl Client {
    pub async fn upgrade_status(&self, package: Option<&str>) -> Result<UpgradeStatus, CliError> {
        let path = match package {
            Some(pkg) => {
                let encoded = encode_segment(pkg)?;
                format!("api/v1/splunk-packages/upgrade/status?package={encoded}")
            }
            None => "api/v1/splunk-packages/upgrade/status".to_string(),
        };
        self.get_json(&path).await
    }

    /// Create a Pending upgrade plan (never auto-approves).
    pub async fn create_upgrade_plan(
        &self,
        package: Option<&str>,
        target_version: Option<&str>,
        targets: &[String],
        name: Option<&str>,
    ) -> Result<ChangePlan, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            package: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            target_version: Option<&'a str>,
            #[serde(skip_serializing_if = "<[String]>::is_empty")]
            targets: &'a [String],
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
        }
        let bytes = self
            .request_bytes(
                reqwest::Method::POST,
                "api/v1/splunk-packages/upgrade/plan",
                Some(&Body {
                    package,
                    target_version,
                    targets,
                    name,
                }),
            )
            .await?;
        if let Ok(envelope) = serde_json::from_slice::<ChangePlanResponse>(&bytes) {
            if let Some(plan) = envelope.plan {
                return Ok(plan);
            }
            if !envelope.success && !envelope.message.is_empty() {
                return Err(CliError::Other(envelope.message));
            }
        }
        serde_json::from_slice::<ChangePlan>(&bytes)
            .map_err(|e| CliError::Transport(format!("invalid upgrade plan JSON: {e}")))
    }
}

fn encode_segment(raw: &str) -> Result<String, CliError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control()) {
        return Err(CliError::InvalidInput(
            "package name must be non-empty".into(),
        ));
    }
    Ok(trimmed
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' => c.to_string(),
            _ => format!("%{:02X}", u32::from(c)),
        })
        .collect())
}
