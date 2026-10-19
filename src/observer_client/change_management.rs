use serde::Deserialize;

use super::Client;
use crate::change_management::MODE_GIT_BACKED;
use crate::errors::CliError;

#[derive(Debug, Deserialize)]
struct ChangeManagementSettingsResponse {
    mode: String,
}

impl Client {
    /// `GET /api/v1/admin/change-management-settings` (requires `read` scope).
    pub async fn get_change_management_mode(&self) -> Result<String, CliError> {
        let resp: ChangeManagementSettingsResponse = self
            .get_json("api/v1/admin/change-management-settings")
            .await?;
        Ok(resp.mode)
    }

    /// Fail before an expensive bundle upload when the tenant is git-backed.
    pub async fn ensure_bundle_plan_allowed(&self) -> Result<(), CliError> {
        match self.get_change_management_mode().await {
            Ok(mode) if mode == MODE_GIT_BACKED => Err(CliError::GitBackedChangeManagement(
                crate::change_management::bundle_blocked_message(),
            )),
            Ok(_) => Ok(()),
            Err(_) => Ok(()),
        }
    }
}
