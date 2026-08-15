use reqwest::Method;
use serde::Serialize;

use super::types;
use super::Client;
use crate::errors::CliError;
use crate::observer_client::ChangePlan;

impl Client {
    /// Direct mode: create a git-sourced draft (CI token path).
    /// Observer `CreateChangePlanRequest` is `deny_unknown_fields`.
    pub async fn create_plan_from_git(
        &self,
        repository_url: &str,
        commit_sha: &str,
        target_group_id: &str,
        name: Option<&str>,
    ) -> Result<ChangePlan, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            source_type: &'a str,
            repository_url: &'a str,
            commit_sha: &'a str,
            target_group_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
        }

        let body = Body {
            source_type: "git",
            repository_url,
            commit_sha,
            target_group_id,
            name,
        };
        let resp: types::ChangePlanResponse = self
            .request_json(Method::POST, "api/v1/plans", Some(&body))
            .await?;
        match resp.plan {
            Some(plan) if resp.success => Ok(plan),
            _ => Err(CliError::Other(format!(
                "plan creation failed: {}",
                resp.message
            ))),
        }
    }

    /// Direct mode: create a draft plan compiled from an uploaded bundle.
    pub async fn create_plan_from_bundle(
        &self,
        bundle_id: &str,
        target_group_id: &str,
        name: Option<&str>,
    ) -> Result<ChangePlan, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            source_type: &'a str,
            source_bundle_id: &'a str,
            target_group_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
        }

        let body = Body {
            source_type: "bundle",
            source_bundle_id: bundle_id,
            target_group_id,
            name,
        };
        let resp: types::ChangePlanResponse = self
            .request_json(Method::POST, "api/v1/plans", Some(&body))
            .await?;
        match resp.plan {
            Some(plan) if resp.success => Ok(plan),
            _ => Err(CliError::Other(format!(
                "plan creation failed: {}",
                resp.message
            ))),
        }
    }
}
