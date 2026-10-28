//! Catalog blueprints (multi-target plan + rollback).

use serde::{Deserialize, Serialize};

use super::types::{ChangePlan, ChangePlanResponse};
use super::Client;
use crate::errors::CliError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Deserialize)]
struct ListCatalogResponse {
    #[serde(default)]
    catalog: Vec<CatalogEntry>,
    #[serde(default)]
    items: Vec<CatalogEntry>,
}

impl Client {
    pub async fn list_catalog(&self) -> Result<Vec<CatalogEntry>, CliError> {
        let bytes = self
            .request_bytes(reqwest::Method::GET, "api/v1/catalog", None::<&()>)
            .await?;
        if let Ok(list) = serde_json::from_slice::<Vec<CatalogEntry>>(&bytes) {
            return Ok(list);
        }
        let wrapped: ListCatalogResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("invalid catalog JSON: {e}")))?;
        if !wrapped.catalog.is_empty() {
            Ok(wrapped.catalog)
        } else {
            Ok(wrapped.items)
        }
    }

    pub async fn show_catalog_entry(&self, id: &str) -> Result<CatalogEntry, CliError> {
        let encoded = encode_segment(id)?;
        let path = format!("api/v1/catalog/{encoded}");
        self.get_json(&path).await
    }

    /// Create Pending plan(s) from a catalog blueprint for the given targets.
    pub async fn plan_from_catalog(
        &self,
        id: &str,
        targets: &[String],
        name: Option<&str>,
    ) -> Result<Vec<ChangePlan>, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            targets: &'a [String],
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
        }
        let encoded = encode_segment(id)?;
        let path = format!("api/v1/catalog/{encoded}/plan");
        let bytes = self
            .request_bytes(reqwest::Method::POST, &path, Some(&Body { targets, name }))
            .await?;

        if let Ok(plans) = serde_json::from_slice::<Vec<ChangePlan>>(&bytes) {
            return Ok(plans);
        }
        if let Ok(envelope) = serde_json::from_slice::<ChangePlanResponse>(&bytes) {
            if let Some(plan) = envelope.plan {
                return Ok(vec![plan]);
            }
            if !envelope.success && !envelope.message.is_empty() {
                return Err(CliError::Other(envelope.message));
            }
        }
        #[derive(Deserialize)]
        struct PlansWrapper {
            #[serde(default)]
            plans: Vec<ChangePlan>,
            #[serde(default)]
            plan: Option<ChangePlan>,
        }
        let wrapped: PlansWrapper = serde_json::from_slice(&bytes)
            .map_err(|e| CliError::Transport(format!("invalid catalog plan JSON: {e}")))?;
        if !wrapped.plans.is_empty() {
            return Ok(wrapped.plans);
        }
        if let Some(plan) = wrapped.plan {
            return Ok(vec![plan]);
        }
        Err(CliError::Transport(
            "catalog plan response did not include a plan".into(),
        ))
    }
}

fn encode_segment(raw: &str) -> Result<String, CliError> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains("..")
        || trimmed.chars().any(|c| c.is_control())
    {
        return Err(CliError::InvalidInput(
            "catalog id must be a non-empty path segment".into(),
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
    fn rejects_unsafe_ids() {
        assert!(encode_segment("").is_err());
        assert!(encode_segment("a/b").is_err());
        assert!(encode_segment("bp-1").is_ok());
    }
}
