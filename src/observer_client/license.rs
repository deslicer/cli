//! Tenant license status / bind.

use serde::{Deserialize, Serialize};

use super::Client;
use crate::errors::CliError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseStatus {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub seats: Option<u64>,
    #[serde(default)]
    pub seats_used: Option<u64>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub bound: Option<bool>,
    #[serde(default)]
    pub license_key_hint: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseBindResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub status: Option<LicenseStatus>,
    #[serde(default)]
    pub message: Option<String>,
}

impl Client {
    pub async fn license_status(&self) -> Result<LicenseStatus, CliError> {
        self.get_json("api/v1/license/status").await
    }

    pub async fn license_bind(&self, key: &str) -> Result<LicenseBindResult, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            key: &'a str,
        }
        self.request_json(
            reqwest::Method::POST,
            "api/v1/license/bind",
            Some(&Body { key }),
        )
        .await
    }
}
