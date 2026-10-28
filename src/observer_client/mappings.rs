//! Environment app mappings (Observer source of truth).

use serde::{Deserialize, Serialize};

use super::Client;
use crate::errors::CliError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentAppMapping {
    pub app: String,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub inventory_group: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Deserialize)]
struct ListMappingsResponse {
    #[serde(default)]
    mappings: Vec<EnvironmentAppMapping>,
    #[serde(default)]
    items: Vec<EnvironmentAppMapping>,
}

impl Client {
    pub async fn list_environment_mappings(
        &self,
        environment: &str,
    ) -> Result<Vec<EnvironmentAppMapping>, CliError> {
        let encoded = encode_env_segment(environment)?;
        let path = format!("api/v1/environments/{encoded}/mappings");
        let bytes = self
            .request_bytes(reqwest::Method::GET, &path, None::<&()>)
            .await?;
        if let Ok(list) = serde_json::from_slice::<Vec<EnvironmentAppMapping>>(&bytes) {
            return Ok(list);
        }
        let wrapped: ListMappingsResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("invalid mappings JSON: {e}")))?;
        if !wrapped.mappings.is_empty() {
            Ok(wrapped.mappings)
        } else {
            Ok(wrapped.items)
        }
    }

    /// Upsert one app mapping for an environment.
    pub async fn set_environment_mapping(
        &self,
        environment: &str,
        mapping: &EnvironmentAppMapping,
    ) -> Result<EnvironmentAppMapping, CliError> {
        let encoded = encode_env_segment(environment)?;
        let path = format!("api/v1/environments/{encoded}/mappings");
        self.request_json(reqwest::Method::PUT, &path, Some(mapping))
            .await
    }

    /// Adopt a live/discovered app into the environment mapping (Observer SoT).
    pub async fn adopt_environment_mapping(
        &self,
        environment: &str,
        app: &str,
        host: Option<&str>,
        inventory_group: Option<&str>,
    ) -> Result<EnvironmentAppMapping, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            app: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            host: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            inventory_group: Option<&'a str>,
        }
        let encoded = encode_env_segment(environment)?;
        let path = format!("api/v1/environments/{encoded}/mappings/adopt");
        self.request_json(
            reqwest::Method::POST,
            &path,
            Some(&Body {
                app,
                host,
                inventory_group,
            }),
        )
        .await
    }
}

fn encode_env_segment(raw: &str) -> Result<String, CliError> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains("..")
        || trimmed.chars().any(|c| c.is_control())
    {
        return Err(CliError::InvalidInput(
            "environment name must be a non-empty path segment".into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_safe_environment_names() {
        assert_eq!(encode_env_segment("prod").unwrap(), "prod");
        assert!(encode_env_segment("").is_err());
        assert!(encode_env_segment("a/b").is_err());
    }
}
