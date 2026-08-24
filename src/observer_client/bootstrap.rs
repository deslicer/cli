use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
pub struct BootstrapTemplates {
    pub provider: String,
    pub tree_sha256: String,
    pub files: Vec<BootstrapTemplateFile>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BootstrapTemplateFile {
    pub path: String,
    pub sha256: String,
    pub contents_base64: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubInstallation {
    pub installation_id: i64,
    pub github_account_login: String,
    pub github_account_type: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct CreateEnvironmentBindingRequest {
    pub installation_id: i64,
    pub github_full_name: String,
    pub environment_name: String,
    pub ci_platform: String,
    pub host_group_id: Uuid,
}
