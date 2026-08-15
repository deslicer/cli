//! Response models matching the Observer API / CI-proxy contracts.
//!
//! Field names mirror `observer-api/src/models/{plans,executions}.rs` and the
//! deslicer-ai CI proxy orchestration responses. Unknown fields are ignored so
//! additive server changes never break the CLI.

use serde::{Deserialize, Serialize};

/// Observer `ChangePlan` (subset). `id` is the internal row ID (UUID v7);
/// `plan_id` is the external identifier (UUID v4) used in API paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePlan {
    pub id: String,
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, alias = "description")]
    pub summary: Option<String>,
}

impl ChangePlan {
    /// External plan id when present, falling back to the row id.
    pub fn external_id(&self) -> &str {
        self.plan_id.as_deref().unwrap_or(&self.id)
    }

    /// Human-readable summary for CI output (falls back to plan name).
    pub fn display_summary(&self) -> String {
        self.summary
            .clone()
            .or_else(|| self.name.clone())
            .unwrap_or_default()
    }
}

/// Observer `PlanProgress` (subset) from `GET /api/v1/plans/{plan_id}/progress`.
/// `progress_status` is one of: not_started, partial, completed, expired.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanProgress {
    pub plan_id: String,
    #[serde(default)]
    pub progress_status: String,
    #[serde(default)]
    pub total_items: i64,
    #[serde(default)]
    pub fully_completed_items: i64,
}

impl PlanProgress {
    pub fn is_terminal(&self) -> bool {
        matches!(self.progress_status.as_str(), "completed" | "expired")
    }
}

/// Observer `ExecutePlanResponse` from `POST /api/v1/plans/{plan_id}/execute`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionQueued {
    pub execution_id: String,
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub jobs_total: i64,
}

/// Observer `ExecutionSummary` (subset) from `GET /api/v1/executions/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub execution_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub jobs_total: i64,
    #[serde(default)]
    pub jobs_succeeded: i64,
    #[serde(default)]
    pub jobs_failed: i64,
    #[serde(default)]
    pub jobs_partial: i64,
    #[serde(default)]
    pub jobs_timed_out: i64,
}

impl ExecutionSummary {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "succeeded" | "partial" | "failed" | "canceled" | "timed_out"
        )
    }

    pub fn is_success(&self) -> bool {
        self.status == "succeeded"
    }
}

/// CI-proxy `POST v1/plan` response: create draft plan + trigger compile.
///
/// `plan_id` is the external identifier for reads/approve/execute;
/// `plan_row_id` is the internal row id for compile-runner calls. Older proxy
/// builds return a single `plan_id` carrying the row id and omit `plan_row_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratedPlan {
    pub plan_id: String,
    #[serde(default)]
    pub plan_row_id: Option<String>,
    #[serde(default)]
    pub status: String,
}

/// CI-proxy `POST v1/plan/verify` response.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OrchestrationVerifyResponse {
    pub accepted: bool,
    pub dry_run: bool,
}

/// Observer `POST /api/v1/plan-sources/bundles` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleUploaded {
    pub id: String,
    pub sha256: String,
    #[serde(default)]
    pub size_bytes: i64,
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Observer `POST /api/v1/plans` response wrapper.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChangePlanResponse {
    pub success: bool,
    #[serde(default)]
    pub message: String,
    pub plan: Option<ChangePlan>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_group_list_and_ignores_unknown_fields() {
        let body = br#"[{"id":"019f36d6-3f61-7eea-9417-7ac4a8a10f69","name":"search-heads","display_name":"Search Heads","member_count":3,"group_type":"role","tenant_id":"11111111-2222-4333-8444-555566667777"}]"#;
        let groups: Vec<HostGroup> = serde_json::from_slice(body).expect("groups");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "019f36d6-3f61-7eea-9417-7ac4a8a10f69");
        assert_eq!(groups[0].name, "search-heads");
        assert_eq!(groups[0].display_name.as_deref(), Some("Search Heads"));
        assert_eq!(groups[0].member_count, Some(3));
    }
}

/// Observer `HostGroupWithCount` subset from `GET /api/v1/groups`.
/// `id` is the value for `change plan --target-group`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub member_count: Option<i64>,
}
