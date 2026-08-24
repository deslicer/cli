//! HTTP client for the Observer API, reached either directly (mgmt plane)
//! or through the deslicer-ai CI proxy (`/api/cli/observer/…`).
//!
//! The client is contract-agnostic about the base URL: `join_api_path` keeps
//! the proxy prefix when the base carries a trailing slash, so the same
//! request paths work in both modes.

mod bootstrap;
mod direct_create;
mod enrollment;
mod http_errors;
mod inventory;
mod types;

pub use bootstrap::{
    BootstrapTemplateFile, BootstrapTemplates, CreateEnvironmentBindingRequest, GithubInstallation,
};
pub use enrollment::{
    CreateEnrollmentTokenRequest, CreateEnrollmentTokenResponse, EnrollmentTokenSummary,
    ListEnrollmentTokensResponse,
};
pub use inventory::InventoryGroup;
pub use types::{
    BundleUploaded, ChangePlan, ExecutionQueued, ExecutionSummary, HostGroup, OrchestratedPlan,
    PlanProgress,
};

use http_errors::{map_observer_error, parse_retry_after_header, retry_delay};

pub(crate) use http_errors::{error_message, parse_retry_after_header as retry_after_header};
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::errors::CliError;
use crate::token_source::TokenSource;

pub struct Client {
    base: url::Url,
    tokens: TokenSource,
    http: reqwest::Client,
    ci_platform: Option<crate::ci::CiPlatform>,
    environment: Option<String>,
}

impl Client {
    pub fn new(base: url::Url, tokens: impl Into<TokenSource>) -> Self {
        Self {
            base,
            tokens: tokens.into(),
            http: crate::http::client(),
            ci_platform: None,
            environment: None,
        }
    }

    pub fn with_ci_platform(mut self, platform: crate::ci::CiPlatform) -> Self {
        self.ci_platform = Some(platform);
        self
    }

    /// Attach `?environment=<env>` to every request. The CI proxy uses it to
    /// pick the environment binding for lifecycle actions; Observer handlers
    /// ignore unknown query params.
    pub fn with_environment(mut self, environment: Option<String>) -> Self {
        self.environment = environment;
        self
    }

    /// CI proxy: create a draft plan bound to this repo/commit and trigger
    /// the ephemeral compile-runner.
    pub async fn create_plan_orchestrated(
        &self,
        environment: Option<&str>,
    ) -> Result<OrchestratedPlan, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            environment: Option<&'a str>,
        }

        let body = Body { environment };
        self.request_json(Method::POST, "v1/plan", Some(&body))
            .await
    }

    /// CI proxy: trigger a dry-run compile for an existing plan.
    /// `plan_row_id` is the internal row id (the compile endpoint's key).
    pub async fn verify_plan_orchestrated(
        &self,
        plan_row_id: &str,
        git_ref: Option<&str>,
    ) -> Result<(), CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            plan_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            git_ref: Option<&'a str>,
        }

        let body = Body {
            plan_id: plan_row_id,
            git_ref,
        };
        let resp: types::OrchestrationVerifyResponse = self
            .request_json(Method::POST, "v1/plan/verify", Some(&body))
            .await?;
        if resp.accepted && resp.dry_run {
            Ok(())
        } else {
            Err(CliError::Other("compile dry-run was not accepted".into()))
        }
    }

    /// Direct mode: upload a source bundle (gzip bytes) for GitHub-App-free
    /// compilation. The declared SHA-256 is verified server-side
    /// (REQ-SIGN-008).
    pub async fn upload_bundle(
        &self,
        bytes: Vec<u8>,
        sha256: &str,
        source_label: Option<&str>,
    ) -> Result<BundleUploaded, CliError> {
        let url = self.request_url("api/v1/plan-sources/bundles")?;
        let bearer = self.tokens.bearer().await?;

        let mut req = self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {bearer}"))
            .header("Content-Type", "application/gzip")
            .header("X-Bundle-Sha256", sha256);
        if let Some(label) = source_label {
            req = req.header("X-Bundle-Source-Label", label);
        }

        let response = req
            .body(bytes)
            .send()
            .await
            .map_err(|e| CliError::Transport(e.to_string()))?;

        let status = response.status();
        let retry_after = parse_retry_after_header(response.headers());
        let body = response
            .bytes()
            .await
            .map_err(|e| CliError::Transport(e.to_string()))?;

        if !status.is_success() {
            let body_text = String::from_utf8_lossy(&body).into_owned();
            return Err(map_observer_error(status, &body_text, retry_after));
        }

        serde_json::from_slice(&body)
            .map_err(|e| CliError::Transport(format!("invalid bundle upload JSON: {e}")))
    }

    /// Direct mode: trigger the compile-runner for a plan (internal row id).
    /// For bundle-sourced plans the `git_ref` is a placeholder — the source
    /// is pinned by the bundle digest, not a git ref.
    pub async fn trigger_compile(&self, plan_row_id: &str, git_ref: &str) -> Result<(), CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            git_ref: &'a str,
        }

        let path = format!("api/v1/runners/compile/{plan_row_id}");
        let _: serde_json::Value = self
            .request_json(Method::POST, &path, Some(&Body { git_ref }))
            .await?;
        Ok(())
    }

    /// Lookup by external plan id (UUID v4).
    pub async fn get_plan(&self, plan_id: &str) -> Result<ChangePlan, CliError> {
        let path = format!("api/v1/plans/{plan_id}");
        self.get_json(&path).await
    }

    pub async fn list_plans(&self, environment: Option<&str>) -> Result<Vec<ChangePlan>, CliError> {
        let mut path = "api/v1/plans".to_string();
        if let Some(env) = environment {
            let encoded = url::form_urlencoded::byte_serialize(env.as_bytes()).collect::<String>();
            path.push_str(&format!("?environment={encoded}"));
        }
        self.get_plans(&path).await
    }

    pub async fn approve(&self, plan_id: &str) -> Result<ChangePlan, CliError> {
        let path = format!("api/v1/plans/{plan_id}/approve");
        self.post_change_plan_mutation(&path).await
    }

    pub async fn reject(&self, plan_id: &str, reason: &str) -> Result<ChangePlan, CliError> {
        #[derive(Serialize)]
        struct RejectBody<'a> {
            reason: &'a str,
        }
        let path = format!("api/v1/plans/{plan_id}/reject");
        let bytes = self
            .request_bytes(Method::POST, &path, Some(&RejectBody { reason }))
            .await?;
        parse_change_plan_body(&bytes)
    }

    /// Observer approve/reject return `{ success, message, plan }`, not a bare plan.
    async fn post_change_plan_mutation(&self, path: &str) -> Result<ChangePlan, CliError> {
        let bytes = self.request_bytes(Method::POST, path, None::<&()>).await?;
        parse_change_plan_body(&bytes)
    }

    /// Queue execution of an approved plan (external plan id).
    pub async fn execute(&self, plan_id: &str) -> Result<ExecutionQueued, CliError> {
        let path = format!("api/v1/plans/{plan_id}/execute");
        self.request_json(Method::POST, &path, None::<&()>).await
    }

    /// Item-completion progress for a plan (external plan id).
    pub async fn progress(&self, plan_id: &str) -> Result<PlanProgress, CliError> {
        let path = format!("api/v1/plans/{plan_id}/progress");
        self.get_json(&path).await
    }

    /// Execution rollout summary (execution id from `execute`).
    pub async fn get_execution(&self, execution_id: &str) -> Result<ExecutionSummary, CliError> {
        let path = format!("api/v1/executions/{execution_id}");
        self.get_json(&path).await
    }

    /// Dry-run diff persisted by the compile-runner (internal plan row id).
    pub async fn get_dry_run_diff(&self, plan_row_id: &str) -> Result<serde_json::Value, CliError> {
        let path = format!("api/v1/plans/{plan_row_id}/diff");
        self.get_json(&path).await
    }

    /// Host groups for `change plan --target-group`.
    pub async fn list_groups(&self) -> Result<Vec<HostGroup>, CliError> {
        self.get_json("api/v1/groups").await
    }

    /// Ansible inventory groups from `GET /api/v1/inventory`.
    pub async fn list_inventory(&self) -> Result<Vec<InventoryGroup>, CliError> {
        let inventory: inventory::AnsibleInventory = self.get_json("api/v1/inventory").await?;
        Ok(inventory.into_groups())
    }

    pub async fn fetch_bootstrap_templates(
        &self,
        provider: &str,
    ) -> Result<BootstrapTemplates, CliError> {
        let path = format!("api/v1/bootstrap-templates?provider={provider}");
        self.get_json(&path).await
    }

    pub async fn list_github_installations(&self) -> Result<Vec<GithubInstallation>, CliError> {
        #[derive(Deserialize)]
        struct ListInstallationsResponse {
            installations: Vec<GithubInstallation>,
        }
        let resp: ListInstallationsResponse =
            self.get_json("api/v1/admin/github-installations").await?;
        Ok(resp.installations)
    }

    pub async fn create_environment_binding(
        &self,
        body: &CreateEnvironmentBindingRequest,
    ) -> Result<bool, CliError> {
        let (status, body_text) = self
            .request_status(
                Method::POST,
                "api/v1/admin/ci-environment-bindings",
                Some(body),
            )
            .await?;
        if status.is_success() {
            return Ok(false);
        }
        if status == reqwest::StatusCode::CONFLICT {
            return Ok(true);
        }
        Err(map_observer_error(status, &body_text, None))
    }

    pub async fn create_enrollment_token(
        &self,
        body: &CreateEnrollmentTokenRequest,
    ) -> Result<CreateEnrollmentTokenResponse, CliError> {
        self.request_json(Method::POST, "api/v1/enrollment-tokens", Some(body))
            .await
    }

    pub async fn list_enrollment_tokens(&self) -> Result<ListEnrollmentTokensResponse, CliError> {
        self.get_json("api/v1/enrollment-tokens").await
    }

    pub async fn revoke_enrollment_token(&self, jti: &uuid::Uuid) -> Result<(), CliError> {
        let path = format!("api/v1/enrollment-tokens/{jti}");
        self.request_bytes(Method::DELETE, &path, None::<&()>)
            .await?;
        Ok(())
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, CliError> {
        self.request_json(Method::GET, path, None::<&()>).await
    }

    async fn get_plans(&self, path: &str) -> Result<Vec<ChangePlan>, CliError> {
        let bytes = self.request_bytes(Method::GET, path, None::<&()>).await?;
        if let Ok(plans) = serde_json::from_slice::<Vec<ChangePlan>>(&bytes) {
            return Ok(plans);
        }
        #[derive(Deserialize)]
        struct PlansWrapper {
            plans: Vec<ChangePlan>,
        }
        serde_json::from_slice::<PlansWrapper>(&bytes)
            .map(|w| w.plans)
            .map_err(|e| CliError::Transport(format!("invalid plans JSON: {e}")))
    }

    async fn request_json<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, CliError>
    where
        T: for<'de> Deserialize<'de>,
        B: Serialize + ?Sized,
    {
        let bytes = self.request_bytes(method, path, body).await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("invalid JSON response: {e}")))
    }

    async fn request_bytes<B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<Vec<u8>, CliError>
    where
        B: Serialize + ?Sized,
    {
        const MAX_ATTEMPTS: u32 = 3;
        const BACKOFF_BASE_MS: u64 = 500;

        let url = self.request_url(path)?;
        let mut bearer = self.tokens.bearer().await?;
        let mut attempt = 0u32;
        let mut auth_refreshed = false;

        loop {
            attempt += 1;
            let mut req = self
                .http
                .request(method.clone(), url.clone())
                .header("Authorization", format!("Bearer {bearer}"));
            if let Some(platform) = self.ci_platform {
                req = req.header("X-Deslicer-CI-Platform", platform.header_value());
            }
            if let Some(payload) = body {
                req = req.json(payload);
            }

            let response = req
                .send()
                .await
                .map_err(|e| CliError::Transport(e.to_string()))?;

            let status = response.status();

            // Short-lived CI OIDC JWTs can expire mid-command: refresh once
            // and retry before surfacing the 401.
            if status == reqwest::StatusCode::UNAUTHORIZED && !auth_refreshed {
                auth_refreshed = true;
                if let Some(fresh) = self.tokens.force_refresh().await? {
                    bearer = fresh;
                    continue;
                }
            }

            if (status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS)
                && attempt < MAX_ATTEMPTS
            {
                let delay = retry_delay(response.headers(), attempt, BACKOFF_BASE_MS);
                tokio::time::sleep(delay).await;
                continue;
            }

            let retry_after = parse_retry_after_header(response.headers());
            let bytes = response
                .bytes()
                .await
                .map_err(|e| CliError::Transport(e.to_string()))?;

            if status.is_success() {
                return Ok(bytes.to_vec());
            }

            let body_text = String::from_utf8_lossy(&bytes).into_owned();
            return Err(map_observer_error(status, &body_text, retry_after));
        }
    }

    async fn request_status<B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<(reqwest::StatusCode, String), CliError>
    where
        B: Serialize + ?Sized,
    {
        let url = self.request_url(path)?;
        let bearer = self.tokens.bearer().await?;
        let mut req = self
            .http
            .request(method, url)
            .header("Authorization", format!("Bearer {bearer}"));
        if let Some(platform) = self.ci_platform {
            req = req.header("X-Deslicer-CI-Platform", platform.header_value());
        }
        if let Some(payload) = body {
            req = req.json(payload);
        }
        let response = req
            .send()
            .await
            .map_err(|e| CliError::Transport(e.to_string()))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| CliError::Transport(e.to_string()))?;
        Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
    }

    fn request_url(&self, path: &str) -> Result<url::Url, CliError> {
        let mut url = self
            .base
            .join(path)
            .map_err(|e| CliError::Transport(format!("invalid URL join: {e}")))?;
        crate::http::assert_url_allowed(&url)?;
        if let Some(env) = &self.environment {
            url.query_pairs_mut().append_pair("environment", env);
        }
        Ok(url)
    }
}

fn parse_change_plan_body(bytes: &[u8]) -> Result<ChangePlan, CliError> {
    if let Ok(envelope) = serde_json::from_slice::<types::ChangePlanResponse>(bytes) {
        if let Some(plan) = envelope.plan {
            return Ok(plan);
        }
    }
    serde_json::from_slice::<ChangePlan>(bytes)
        .map_err(|e| CliError::Transport(format!("invalid plan JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wrapped_approve_response() {
        let body = br#"{"success":true,"message":"ok","plan":{"id":"row","plan_id":"ext","status":"approved_unsigned"}}"#;
        let plan = parse_change_plan_body(body).expect("plan");
        assert_eq!(plan.external_id(), "ext");
        assert_eq!(plan.status, "approved_unsigned");
    }
}
