use serde::{Deserialize, Serialize};

use crate::errors::CliError;

use super::Client;

#[derive(Debug, Serialize)]
pub struct ProvisionRepoRequest {
    pub repo_name: String,
    pub visibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProvisionRepoResponse {
    pub repo_full_name: String,
    pub html_url: String,
    pub github_repo_id: i64,
    pub default_branch: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GithubRepoRow {
    pub github_repo_id: i64,
    pub github_full_name: String,
    pub added_at: String,
    pub bootstrap_pr_url: Option<String>,
    pub bootstrap_pr_state: Option<String>,
    pub workflow_template_digest: Option<String>,
    pub workflow_drift_checked_at: Option<String>,
    pub workflows_in_sync: Option<bool>,
    #[serde(default)]
    pub workflow_refresh_pending: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListReposResponse {
    pub repos: Vec<GithubRepoRow>,
    pub embedded_workflow_template_digest: String,
}

impl Client {
    pub async fn provision_github_repo(
        &self,
        installation_id: i64,
        body: &ProvisionRepoRequest,
    ) -> Result<ProvisionRepoResponse, CliError> {
        let path = format!("api/v1/admin/github-installations/{installation_id}/repos/provision");
        self.request_json(reqwest::Method::POST, &path, Some(body))
            .await
    }

    pub async fn refresh_repo_workflows(
        &self,
        installation_id: i64,
        repo_id: i64,
    ) -> Result<(), CliError> {
        let path = format!(
            "api/v1/admin/github-installations/{installation_id}/repos/{repo_id}/workflows/refresh"
        );
        self.request_bytes(reqwest::Method::POST, &path, None::<&()>)
            .await?;
        Ok(())
    }

    pub async fn list_github_repos(
        &self,
        installation_id: i64,
    ) -> Result<ListReposResponse, CliError> {
        let path = format!("api/v1/admin/github-installations/{installation_id}/repos");
        self.get_json(&path).await
    }
}
