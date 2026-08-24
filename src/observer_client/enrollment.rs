use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct CreateEnrollmentTokenRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_hosts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind_to_host_id: Option<Uuid>,
    pub token_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateEnrollmentTokenResponse {
    pub token: String,
    pub jti: Uuid,
    pub tenant_id: Uuid,
    pub max_hosts: u32,
    pub expires_at: String,
    pub api_key_ttl_days: u32,
    pub bind_to_host_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EnrollmentTokenSummary {
    pub jti: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub max_hosts: i32,
    pub enrolled_count: i64,
    pub expires_at: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
    pub is_active: bool,
    pub token_purpose: String,
    pub bind_to_host_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListEnrollmentTokensResponse {
    pub tokens: Vec<EnrollmentTokenSummary>,
    pub total: i64,
}
