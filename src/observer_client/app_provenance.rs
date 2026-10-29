//! App provenance / insights apps (inventory apps list + adopt).

use serde::{Deserialize, Serialize};

use super::Client;
use crate::errors::CliError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryApp {
    /// Observer sometimes omits `name` or sends `app_name` (insights vs
    /// provenance). Keep decode tolerant so list does not fail as transport.
    #[serde(default, alias = "app_name")]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub inventory_group: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
}

#[derive(Deserialize)]
struct AppsListResponse {
    #[serde(default)]
    apps: Vec<InventoryApp>,
    #[serde(default)]
    items: Vec<InventoryApp>,
}

impl Client {
    /// List apps from insights / provenance, optionally filtered by host or group.
    pub async fn list_inventory_apps(
        &self,
        host: Option<&str>,
        group: Option<&str>,
    ) -> Result<Vec<InventoryApp>, CliError> {
        let mut path = String::from("api/v1/insights/apps");
        let mut query = Vec::new();
        if let Some(host) = host {
            query.push(format!("host={}", encode_query(host)));
        }
        if let Some(group) = group {
            query.push(format!("group={}", encode_query(group)));
        }
        if !query.is_empty() {
            path.push('?');
            path.push_str(&query.join("&"));
        }

        match self.fetch_apps_list(&path).await {
            Ok(apps) => Ok(apps),
            Err(primary) => {
                // Fallback to app-provenance list when insights is unavailable.
                let mut fallback = String::from("api/v1/app-provenance");
                if !query.is_empty() {
                    fallback.push('?');
                    fallback.push_str(&query.join("&"));
                }
                self.fetch_apps_list(&fallback).await.or(Err(primary))
            }
        }
    }

    /// Adopt a live app into provenance / environment mapping for a host.
    pub async fn adopt_inventory_app(
        &self,
        app: &str,
        host: &str,
        group: Option<&str>,
    ) -> Result<InventoryApp, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            app: &'a str,
            host: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            inventory_group: Option<&'a str>,
        }
        self.request_json(
            reqwest::Method::POST,
            "api/v1/app-provenance/adopt",
            Some(&Body {
                app,
                host,
                inventory_group: group,
            }),
        )
        .await
    }

    async fn fetch_apps_list(&self, path: &str) -> Result<Vec<InventoryApp>, CliError> {
        let bytes = self
            .request_bytes(reqwest::Method::GET, path, None::<&()>)
            .await?;
        if let Ok(list) = serde_json::from_slice::<Vec<InventoryApp>>(&bytes) {
            return Ok(list);
        }
        let wrapped: AppsListResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("invalid apps JSON: {e}")))?;
        if !wrapped.apps.is_empty() {
            Ok(wrapped.apps)
        } else {
            Ok(wrapped.items)
        }
    }
}

fn encode_query(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", u32::from(c)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_apps_when_name_is_omitted() {
        let raw = r#"{"apps":[{"version":"1.0","host":"idx1","status":"discovered"}]}"#;
        let wrapped: AppsListResponse = serde_json::from_str(raw).expect("decode");
        assert_eq!(wrapped.apps.len(), 1);
        assert_eq!(wrapped.apps[0].name, None);
        assert_eq!(wrapped.apps[0].host.as_deref(), Some("idx1"));
    }

    #[test]
    fn decodes_app_name_alias() {
        let raw = r#"{"items":[{"app_name":"ta_nix","host":"sh1"}]}"#;
        let wrapped: AppsListResponse = serde_json::from_str(raw).expect("decode");
        assert_eq!(wrapped.items[0].name.as_deref(), Some("ta_nix"));
    }

    #[test]
    fn decodes_bare_array_without_name() {
        let raw = r#"[{"host":"idx1","inventory_group":"indexers"}]"#;
        let list: Vec<InventoryApp> = serde_json::from_str(raw).expect("decode");
        assert_eq!(list.len(), 1);
        assert!(list[0].name.is_none());
    }
}
