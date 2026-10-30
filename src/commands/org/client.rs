use serde::{Deserialize, Serialize};

use crate::commands::agent::http_errors::map_error_body;
use crate::device_flow::join_api;
use crate::errors::CliError;
use crate::http::try_client;
use crate::session_portal::resolve_deslicer_api_url;
use crate::token_store::{load_active_session, CompositeTokenStore, StoredSession, TokenStore};
use crate::Ctx;

const ORG_PATH: &str = "api/cli/org";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgRole {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgTeam {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub member_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanApprovalSettings {
    #[serde(default)]
    pub require_human_approval: Option<bool>,
    #[serde(default)]
    pub auto_approve: Option<bool>,
    #[serde(default)]
    pub approver_roles: Option<Vec<String>>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanExecutionSettings {
    #[serde(default)]
    pub allow_ci_execute: Option<bool>,
    #[serde(default)]
    pub require_signed_approve: Option<bool>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Deserialize)]
struct ListRolesResponse {
    #[serde(default)]
    roles: Vec<OrgRole>,
}

#[derive(Deserialize)]
struct ListTeamsResponse {
    #[serde(default)]
    teams: Vec<OrgTeam>,
}

#[derive(Deserialize)]
struct SwitchResponse {
    #[serde(flatten)]
    session: Option<StoredSession>,
    #[serde(default)]
    cli_session_token: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    observer_api_url: Option<String>,
    #[serde(default)]
    tenant_slug: Option<String>,
    #[serde(default)]
    deslicer_api_url: Option<String>,
}

pub struct OrgClient {
    base: url::Url,
    token: String,
    http: reqwest::Client,
    session: StoredSession,
}

impl OrgClient {
    pub fn from_ctx(ctx: &Ctx) -> Result<Self, CliError> {
        let session = load_active_session()?.ok_or_else(|| {
            CliError::DeviceSessionRequired(
                "no active device session. Run `deslicer auth login` first \
                 (org commands are not available with a static API token)."
                    .into(),
            )
        })?;
        Ok(Self {
            base: resolve_deslicer_api_url(ctx, &session),
            token: session.cli_session_token.clone(),
            http: try_client()?,
            session,
        })
    }

    pub fn tenant_id(&self) -> &str {
        &self.session.tenant_id
    }

    pub async fn list_roles(&self) -> Result<Vec<OrgRole>, CliError> {
        let url = self.endpoint(&format!("{ORG_PATH}/roles"))?;
        let bytes = self.get_bytes(url, "list roles").await?;
        if let Ok(list) = serde_json::from_slice::<Vec<OrgRole>>(&bytes) {
            return Ok(list);
        }
        let wrapped: ListRolesResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("decode roles: {e}")))?;
        Ok(wrapped.roles)
    }

    pub async fn list_teams(&self) -> Result<Vec<OrgTeam>, CliError> {
        let url = self.endpoint(&format!("{ORG_PATH}/teams"))?;
        let bytes = self.get_bytes(url, "list teams").await?;
        if let Ok(list) = serde_json::from_slice::<Vec<OrgTeam>>(&bytes) {
            return Ok(list);
        }
        let wrapped: ListTeamsResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("decode teams: {e}")))?;
        Ok(wrapped.teams)
    }

    pub async fn switch_tenant(&self, tenant: &str) -> Result<StoredSession, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            tenant: &'a str,
        }
        let url = self.endpoint(&format!("{ORG_PATH}/switch"))?;
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.token)
            .json(&Body { tenant })
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("org switch: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        let body: SwitchResponse = response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode org switch: {e}")))?;
        if let Some(session) = body.session {
            CompositeTokenStore::default_store()?.save(&session)?;
            return Ok(session);
        }
        let session = StoredSession {
            cli_session_token: body.cli_session_token.ok_or_else(|| {
                CliError::Transport("org switch response missing cli_session_token".into())
            })?,
            expires_at: body
                .expires_at
                .unwrap_or_else(|| self.session.expires_at.clone()),
            tenant_id: body.tenant_id.unwrap_or_else(|| tenant.to_string()),
            display_name: body
                .display_name
                .unwrap_or_else(|| self.session.display_name.clone()),
            observer_api_url: body
                .observer_api_url
                .unwrap_or_else(|| self.session.observer_api_url.clone()),
            tenant_slug: body
                .tenant_slug
                .or_else(|| self.session.tenant_slug.clone()),
            deslicer_api_url: body
                .deslicer_api_url
                .or_else(|| self.session.deslicer_api_url.clone()),
        };
        CompositeTokenStore::default_store()?.save(&session)?;
        Ok(session)
    }

    pub async fn get_approval_settings(&self) -> Result<PlanApprovalSettings, CliError> {
        // DAI org path; Observer exposes the same payload at
        // `/api/v1/tenants/{id}/plan-approval-settings` when reached via proxy.
        let primary = format!("{ORG_PATH}/settings/approval");
        match self.get_json_path::<PlanApprovalSettings>(&primary).await {
            Ok(settings) => Ok(settings),
            Err(primary_err) => {
                let fallback = format!(
                    "api/v1/tenants/{}/plan-approval-settings",
                    encode_segment(self.tenant_id())?
                );
                self.get_json_path(&fallback).await.or(Err(primary_err))
            }
        }
    }

    pub async fn set_approval_settings(
        &self,
        settings: &PlanApprovalSettings,
    ) -> Result<PlanApprovalSettings, CliError> {
        let url = self.endpoint(&format!("{ORG_PATH}/settings/approval"))?;
        let response = self
            .http
            .put(url)
            .bearer_auth(&self.token)
            .json(settings)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("set approval settings: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode approval settings: {e}")))
    }

    pub async fn get_execution_settings(&self) -> Result<PlanExecutionSettings, CliError> {
        let url = self.endpoint(&format!("{ORG_PATH}/settings/execution"))?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("execution settings: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode execution settings: {e}")))
    }

    pub async fn set_execution_settings(
        &self,
        settings: &PlanExecutionSettings,
    ) -> Result<PlanExecutionSettings, CliError> {
        let url = self.endpoint(&format!("{ORG_PATH}/settings/execution"))?;
        let response = self
            .http
            .put(url)
            .bearer_auth(&self.token)
            .json(settings)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("set execution settings: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode execution settings: {e}")))
    }

    async fn get_json_path<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, CliError> {
        let url = self.endpoint(path)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("GET {path}: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| CliError::Transport(format!("decode {path}: {e}")))
    }

    async fn get_bytes(&self, url: url::Url, what: &str) -> Result<Vec<u8>, CliError> {
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("{what}: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| CliError::Transport(format!("{what} body: {e}")))
    }

    fn endpoint(&self, path: &str) -> Result<url::Url, CliError> {
        join_api(&self.base, path)
    }

    async fn error_from(&self, response: reqwest::Response) -> CliError {
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60);
        let body = response.text().await.unwrap_or_default();
        map_error_body(status, &body, retry_after)
    }
}

fn encode_segment(raw: &str) -> Result<String, CliError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains("..") {
        return Err(CliError::InvalidInput(
            "tenant id must be a non-empty path segment".into(),
        ));
    }
    Ok(trimmed
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' => c.to_string(),
            _ => format!("%{:02X}", u32::from(c)),
        })
        .collect())
}
