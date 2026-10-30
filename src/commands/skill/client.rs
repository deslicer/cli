use serde::{Deserialize, Serialize};

use crate::commands::agent::http_errors::map_error_body;
use crate::device_flow::join_api;
use crate::errors::CliError;
use crate::http::try_client;
use crate::session_portal::resolve_deslicer_api_url;
use crate::token_store::load_active_session;
use crate::Ctx;

const SKILLS_PATH: &str = "api/cli/skills";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub renamed_from: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub scan_status: Option<String>,
    #[serde(default)]
    pub scan_findings: Option<u64>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillImportResult {
    #[serde(default)]
    pub skill: Option<SkillSummary>,
    #[serde(default)]
    pub renamed_to: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub scan_status: Option<String>,
    #[serde(default)]
    pub scan_findings: Option<u64>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Deserialize)]
struct ListSkillsResponse {
    #[serde(default)]
    skills: Vec<SkillSummary>,
}

pub struct SkillClient {
    base: url::Url,
    token: String,
    http: reqwest::Client,
}

impl SkillClient {
    pub fn from_ctx(ctx: &Ctx) -> Result<Self, CliError> {
        let session = load_active_session()?.ok_or_else(|| {
            CliError::DeviceSessionRequired(
                "no active device session. Run `deslicer auth login` first \
                 (skills are not available with a static API token)."
                    .into(),
            )
        })?;
        Ok(Self {
            base: resolve_deslicer_api_url(ctx, &session),
            token: session.cli_session_token,
            http: try_client()?,
        })
    }

    pub async fn list(&self) -> Result<Vec<SkillSummary>, CliError> {
        let url = self.endpoint(SKILLS_PATH)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("list skills: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| CliError::Transport(format!("list skills body: {e}")))?;
        if let Ok(list) = serde_json::from_slice::<Vec<SkillSummary>>(&bytes) {
            return Ok(list);
        }
        let wrapped: ListSkillsResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("decode skills: {e}")))?;
        Ok(wrapped.skills)
    }

    pub async fn import(
        &self,
        source: &str,
        name: Option<&str>,
    ) -> Result<SkillImportResult, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            source: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
        }
        let url = self.endpoint(&format!("{SKILLS_PATH}/import"))?;
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.token)
            .json(&Body { source, name })
            .send()
            .await
            .map_err(|e| CliError::Transport(format!("import skill: {e}")))?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| CliError::Transport(format!("import skill body: {e}")))?;
        if let Ok(result) = serde_json::from_slice::<SkillImportResult>(&bytes) {
            return Ok(result);
        }
        if let Ok(skill) = serde_json::from_slice::<SkillSummary>(&bytes) {
            return Ok(SkillImportResult {
                size_bytes: skill.size_bytes,
                scan_status: skill.scan_status.clone(),
                scan_findings: skill.scan_findings,
                renamed_to: skill.renamed_from.clone(),
                skill: Some(skill),
                message: None,
            });
        }
        Err(CliError::Transport("invalid skill import response".into()))
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
