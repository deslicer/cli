//! Environment app mappings (Observer source of truth).

use serde::{Deserialize, Serialize};

use super::Client;
use crate::errors::CliError;

/// One row from `GET/PUT /api/v1/environments/{env}/mappings`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentAppMapping {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(alias = "app")]
    pub app_name: String,
    #[serde(default)]
    pub host_group_id: Option<String>,
    #[serde(default)]
    pub inventory_group: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub dest_dir: Option<String>,
    #[serde(default)]
    pub target_host_id: Option<String>,
    #[serde(default, alias = "host")]
    pub target_host: Option<String>,
    #[serde(default, alias = "status")]
    pub state: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// One entry in the `PUT .../mappings` request body.
#[derive(Debug, Clone, Serialize)]
pub struct MappingUpsert {
    pub host_group_id: String,
    pub app_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_host_id: Option<String>,
}

#[derive(Serialize)]
struct UpsertMappingsRequest<'a> {
    mappings: &'a [MappingUpsert],
    /// Always false: the CLI only upserts; it never deletes sibling rows.
    replace: bool,
}

#[derive(Deserialize)]
struct ListMappingsResponse {
    #[serde(default)]
    mappings: Vec<EnvironmentAppMapping>,
    #[serde(default)]
    items: Vec<EnvironmentAppMapping>,
}

fn parse_mappings(bytes: &[u8]) -> Result<Vec<EnvironmentAppMapping>, CliError> {
    if let Ok(list) = serde_json::from_slice::<Vec<EnvironmentAppMapping>>(bytes) {
        return Ok(list);
    }
    let wrapped: ListMappingsResponse = serde_json::from_slice(bytes)
        .map_err(|e| CliError::Transport(format!("invalid mappings JSON: {e}")))?;
    if !wrapped.mappings.is_empty() {
        Ok(wrapped.mappings)
    } else {
        Ok(wrapped.items)
    }
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
        parse_mappings(&bytes)
    }

    /// Upsert app mappings (`PUT .../mappings` with `replace=false`).
    ///
    /// Observer only accepts direct writes when the environment mapping
    /// source is `db`; repo_yaml / distribution sources return 409.
    /// Returns the full mapping list for the environment after the write.
    pub async fn upsert_environment_mappings(
        &self,
        environment: &str,
        entries: &[MappingUpsert],
    ) -> Result<Vec<EnvironmentAppMapping>, CliError> {
        let encoded = encode_env_segment(environment)?;
        let path = format!("api/v1/environments/{encoded}/mappings");
        let body = UpsertMappingsRequest {
            mappings: entries,
            replace: false,
        };
        let bytes = self
            .request_bytes(reqwest::Method::PUT, &path, Some(&body))
            .await?;
        parse_mappings(&bytes)
    }
}

/// Pick the row matching an upserted entry out of the post-write list.
pub fn find_mapping<'a>(
    mappings: &'a [EnvironmentAppMapping],
    entry: &MappingUpsert,
) -> Option<&'a EnvironmentAppMapping> {
    mappings.iter().find(|m| {
        m.app_name == entry.app_name
            && m.host_group_id.as_deref() == Some(entry.host_group_id.as_str())
    })
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

    #[test]
    fn parses_observer_list_shape_with_app_name() {
        let body = br#"{"environment_name":"prod","mappings":[{"id":"1","environment_name":"prod","inventory_group":"idx","host_group_id":"g1","app_name":"ta_nix","source_path":"apps/ta_nix","dest_dir":null,"target_host_id":null,"target_host":null,"state":"present","source":"db","created_at":"x","updated_at":"y"}]}"#;
        let rows = parse_mappings(body).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].app_name, "ta_nix");
        assert_eq!(rows[0].state.as_deref(), Some("present"));
    }

    #[test]
    fn upsert_body_never_replaces() {
        let entries = [MappingUpsert {
            host_group_id: "g1".into(),
            app_name: "ta_nix".into(),
            source_path: None,
            dest_dir: None,
            target_host_id: None,
        }];
        let v = serde_json::to_value(UpsertMappingsRequest {
            mappings: &entries,
            replace: false,
        })
        .unwrap();
        assert_eq!(v["replace"], false);
        assert_eq!(v["mappings"][0]["app_name"], "ta_nix");
        assert!(v["mappings"][0].get("source_path").is_none());
    }
}
