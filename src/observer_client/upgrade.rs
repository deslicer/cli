//! Splunk upgrade intents against Observer `/api/v1/splunk-packages/*`.
//!
//! - readiness → `POST bootstrap/evaluate` (read-only heartbeat evaluation)
//! - preview   → `POST upgrade-rolling/schedule` (read-only, no change_plan insert)
//! - plan      → `POST upgrade-rolling` (creates a Pending plan; never approves)

use serde::{Deserialize, Serialize};

use super::Client;
use crate::errors::CliError;

/// Rolling upgrade families accepted by Observer.
pub const UPGRADE_FAMILIES: &[&str] = &["upgrade_rolling_clusters", "upgrade_non_clustered"];

/// Role-step overlay groups accepted by Observer (`enabled_role_groups`).
pub const UPGRADE_ROLE_GROUPS: &[&str] = &[
    "precheck",
    "cluster_managers",
    "dcn_mc",
    "searchhead",
    "index_cluster",
    "non_clustered",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrolledHost {
    pub host_id: String,
    #[serde(default)]
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeReadiness {
    pub path: String,
    #[serde(default)]
    pub needs_readiness_plan: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub pin_version: Option<String>,
    #[serde(default)]
    pub installed_version: Option<String>,
    #[serde(default)]
    pub strict_x509_required: bool,
    #[serde(default)]
    pub strict_x509_passed: Option<bool>,
    #[serde(default)]
    pub captain_stability: Option<String>,
}

/// Shared body for schedule preview and plan compile.
#[derive(Debug, Clone, Serialize)]
pub struct UpgradeRollingRequest {
    pub family: String,
    pub host_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captain_hostname: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enabled_role_groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeWave {
    pub wave_index: usize,
    pub step_id: String,
    #[serde(default)]
    pub host_scope: String,
    #[serde(default)]
    pub serial_policy: String,
    #[serde(default)]
    pub host_ids: Vec<String>,
    #[serde(default)]
    pub sub_batch_count: usize,
    #[serde(default)]
    pub publishable: bool,
    #[serde(default)]
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradePreview {
    pub family: String,
    #[serde(default)]
    pub catalog_revision: String,
    #[serde(default)]
    pub applied_overlay_ids: Vec<String>,
    pub to_version: String,
    #[serde(default)]
    pub publishable_wave_index: Option<usize>,
    #[serde(default)]
    pub waves: Vec<UpgradeWave>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradePlanCreated {
    pub plan_id: String,
    pub family: String,
    #[serde(default)]
    pub catalog_revision: String,
    #[serde(default)]
    pub phase_group_ids: Vec<String>,
    #[serde(default)]
    pub phase_ids: Vec<String>,
    #[serde(default)]
    pub host_ids: Vec<String>,
    #[serde(default)]
    pub to_version: String,
}

impl Client {
    /// Enrolled hosts (`GET /api/v1/hosts`), used to resolve `--target`.
    pub async fn list_enrolled_hosts(&self) -> Result<Vec<EnrolledHost>, CliError> {
        self.get_json("api/v1/hosts").await
    }

    pub async fn upgrade_readiness(&self, host_id: &str) -> Result<UpgradeReadiness, CliError> {
        self.request_json(
            reqwest::Method::POST,
            "api/v1/splunk-packages/bootstrap/evaluate",
            Some(&serde_json::json!({ "host_id": host_id })),
        )
        .await
    }

    pub async fn upgrade_preview(
        &self,
        body: &UpgradeRollingRequest,
    ) -> Result<UpgradePreview, CliError> {
        self.request_json(
            reqwest::Method::POST,
            "api/v1/splunk-packages/upgrade-rolling/schedule",
            Some(body),
        )
        .await
    }

    /// Create a Pending rolling upgrade plan. Never approves or deploys.
    pub async fn create_upgrade_plan(
        &self,
        body: &UpgradeRollingRequest,
    ) -> Result<UpgradePlanCreated, CliError> {
        self.request_json(
            reqwest::Method::POST,
            "api/v1/splunk-packages/upgrade-rolling",
            Some(body),
        )
        .await
    }
}

/// Resolve `--target` values (hostname, case-insensitive, or host UUID) to
/// host UUIDs. Unknown or ambiguous targets are `InvalidInput` (exit 2).
pub fn resolve_targets(
    hosts: &[EnrolledHost],
    targets: &[String],
) -> Result<Vec<String>, CliError> {
    let mut out = Vec::new();
    for target in targets {
        let t = target.trim();
        if t.is_empty() {
            return Err(CliError::InvalidInput("--target must not be empty".into()));
        }
        let matches: Vec<&EnrolledHost> = hosts
            .iter()
            .filter(|h| {
                h.host_id.eq_ignore_ascii_case(t)
                    || h.hostname
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(t))
            })
            .collect();
        match matches.as_slice() {
            [one] => {
                if !out.contains(&one.host_id) {
                    out.push(one.host_id.clone());
                }
            }
            [] => {
                return Err(CliError::InvalidInput(format!(
                    "--target {t:?} matches no enrolled host"
                )))
            }
            _ => {
                return Err(CliError::InvalidInput(format!(
                    "--target {t:?} matches {} hosts; pass the host UUID",
                    matches.len()
                )))
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts() -> Vec<EnrolledHost> {
        vec![
            EnrolledHost {
                host_id: "h-1".into(),
                hostname: Some("sh1".into()),
            },
            EnrolledHost {
                host_id: "h-2".into(),
                hostname: Some("idx1".into()),
            },
        ]
    }

    #[test]
    fn resolves_hostname_and_uuid() {
        let ids = resolve_targets(&hosts(), &["SH1".into(), "h-2".into(), "sh1".into()]).unwrap();
        assert_eq!(ids, vec!["h-1", "h-2"]);
    }

    #[test]
    fn unknown_target_is_invalid_input() {
        assert!(matches!(
            resolve_targets(&hosts(), &["nope".into()]),
            Err(CliError::InvalidInput(_))
        ));
    }

    #[test]
    fn plan_body_omits_empty_optionals() {
        let body = UpgradeRollingRequest {
            family: "upgrade_non_clustered".into(),
            host_ids: vec!["h-1".into()],
            to_version: Some("9.4.1".into()),
            captain_hostname: None,
            enabled_role_groups: vec![],
        };
        let v = serde_json::to_value(&body).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"family":"upgrade_non_clustered","host_ids":["h-1"],"to_version":"9.4.1"})
        );
    }
}
