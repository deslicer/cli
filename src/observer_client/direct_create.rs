use reqwest::Method;
use serde::Deserialize;
use serde::Serialize;

use super::http_errors::{is_duplicate_plan_error, map_observer_error};
use super::types;
use super::Client;
use crate::errors::CliError;
use crate::observer_client::ChangePlan;

impl Client {
    /// Direct mode: create a git-sourced draft (CI token path).
    /// Observer `CreateChangePlanRequest` is `deny_unknown_fields`.
    ///
    /// A 409 `duplicate_plan` from current Observer is treated as reuse:
    /// search for the active row and return it (REQ-API-001).
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
        let (status, body_text) = self
            .request_status(Method::POST, "api/v1/plans", Some(&body))
            .await?;
        if status == reqwest::StatusCode::CONFLICT && is_duplicate_plan_error(&body_text) {
            return self
                .find_existing_plan_for_commit(repository_url, commit_sha)
                .await;
        }
        if !status.is_success() {
            return Err(map_observer_error(status, &body_text, None));
        }
        parse_created_plan(&body_text)
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

    async fn find_existing_plan_for_commit(
        &self,
        repository_url: &str,
        commit_sha: &str,
    ) -> Result<ChangePlan, CliError> {
        #[derive(Serialize)]
        struct SearchBody<'a> {
            filters: SearchFilters<'a>,
        }
        #[derive(Serialize)]
        struct SearchFilters<'a> {
            repository_url: &'a str,
            commit_sha: &'a str,
        }
        #[derive(Deserialize)]
        struct SearchResponse {
            plans: Vec<ChangePlan>,
        }

        let body = SearchBody {
            filters: SearchFilters {
                repository_url,
                commit_sha,
            },
        };
        let resp: SearchResponse = self
            .request_json(Method::POST, "api/v1/plans/search", Some(&body))
            .await?;
        resp.plans.into_iter().next().ok_or_else(|| {
            CliError::Other(
                "An active plan already exists for this repository and commit, \
                 but search returned no matching row"
                    .into(),
            )
        })
    }
}

fn parse_created_plan(body_text: &str) -> Result<ChangePlan, CliError> {
    let resp: types::ChangePlanResponse = serde_json::from_str(body_text)
        .map_err(|e| CliError::Transport(format!("invalid plan JSON: {e}")))?;
    match resp.plan {
        Some(plan) if resp.success => Ok(plan),
        _ => Err(CliError::Other(format!(
            "plan creation failed: {}",
            resp.message
        ))),
    }
}
