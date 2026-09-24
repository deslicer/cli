//! Declared hosts + inventory assign (device-session / admin paths).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Slim Observer `Host` row for declared-host create responses.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DeclaredHost {
    /// Internal `hosts.id` — what `POST /api/v1/inventory/assign` expects.
    pub id: Uuid,
    pub host_id: Uuid,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateDeclaredHostRequest<'a> {
    pub hostname: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansible_host: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssignHostRequest {
    pub host_id: Uuid,
    pub role: String,
    pub availability_group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_number: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct InventoryAssignmentResponse {
    pub success: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub ansible_group_name: Option<String>,
}
