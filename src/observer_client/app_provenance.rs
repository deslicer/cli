//! Fleet apps inventory (`GET /api/v1/insights/apps`) and adopt
//! (`POST /api/v1/app-provenance/adopt`).
//!
//! Observer returns one row per app with `installed_on` / `missing_from`
//! host lists (DAI #2858 host filter is applied client-side on those lists).
//! The CLI flattens that to one row per (app, host).

use serde::{Deserialize, Serialize};

use super::Client;
use crate::errors::CliError;

/// Observer `insights/apps` pages hold at most 2000 apps.
const APPS_PAGE_LIMIT: i64 = 2000;
/// Safety stop for pagination (2000 × 50 = 100k apps).
const MAX_APPS_PAGES: usize = 50;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppHostRef {
    pub host_id: String,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub observed_version: Option<String>,
}

impl AppHostRef {
    /// Match by hostname (case-insensitive) or exact host UUID.
    pub fn matches(&self, needle: &str) -> bool {
        let needle = needle.trim();
        self.host_id.eq_ignore_ascii_case(needle)
            || self
                .hostname
                .as_deref()
                .is_some_and(|h| h.eq_ignore_ascii_case(needle))
    }
}

/// Observer `AppInventoryItem`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InsightsApp {
    #[serde(default)]
    pub app_name: String,
    #[serde(default)]
    pub installed_on: Vec<AppHostRef>,
    #[serde(default)]
    pub missing_from: Vec<AppHostRef>,
    #[serde(default)]
    pub observed_version: Option<String>,
    #[serde(default)]
    pub last_seen: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InsightsAppsPage {
    #[serde(default)]
    apps: Vec<InsightsApp>,
    #[serde(default)]
    total: i64,
}

/// One flattened (app, host) row printed by `inventory apps`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventoryApp {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    pub host: String,
    pub host_id: String,
    #[serde(default)]
    pub last_seen: Option<String>,
}

/// Flatten Observer rows to one row per installed host, keeping only hosts in
/// `keep` when it is set.
pub fn flatten_installed(
    apps: &[InsightsApp],
    keep: &dyn Fn(&AppHostRef) -> bool,
) -> Vec<InventoryApp> {
    let mut rows = Vec::new();
    for app in apps {
        for host in app.installed_on.iter().filter(|h| keep(h)) {
            rows.push(InventoryApp {
                name: app.app_name.clone(),
                version: host
                    .observed_version
                    .clone()
                    .or_else(|| app.observed_version.clone()),
                host: host
                    .hostname
                    .clone()
                    .unwrap_or_else(|| host.host_id.clone()),
                host_id: host.host_id.clone(),
                last_seen: app.last_seen.clone(),
            });
        }
    }
    rows
}

/// Body for `POST /api/v1/app-provenance/adopt` (Observer denies unknown fields).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AdoptAppRequest {
    pub scope: String,
    pub app_name: String,
    pub host_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory_group_id: Option<String>,
    /// `managed` = adopt as Git-managed.
    pub management_mode: String,
    pub content_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// Observer `AppProvenanceItem` (subset).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptedApp {
    pub id: String,
    pub app_name: String,
    pub scope: String,
    #[serde(default)]
    pub host_id: Option<String>,
    #[serde(default)]
    pub inventory_group_id: Option<String>,
    pub management_mode: String,
    #[serde(default)]
    pub content_source: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
}

impl Client {
    /// All apps from `GET /api/v1/insights/apps`, following `limit`/`offset`.
    pub async fn list_insights_apps(&self) -> Result<Vec<InsightsApp>, CliError> {
        let mut all = Vec::new();
        for page in 0..MAX_APPS_PAGES {
            let offset = page as i64 * APPS_PAGE_LIMIT;
            let path = format!("api/v1/insights/apps?limit={APPS_PAGE_LIMIT}&offset={offset}");
            let bytes = self
                .request_bytes(reqwest::Method::GET, &path, None::<&()>)
                .await?;
            let page: InsightsAppsPage = serde_json::from_slice(&bytes)
                .map_err(|e| CliError::Transport(format!("invalid insights/apps JSON: {e}")))?;
            let got = page.apps.len() as i64;
            all.extend(page.apps);
            if got < APPS_PAGE_LIMIT || offset + got >= page.total {
                return Ok(all);
            }
        }
        Ok(all)
    }

    /// Bind a live app to provenance (`management_mode=managed` = Git-managed).
    /// Observer requires admin scope and a verified human (device login).
    pub async fn adopt_inventory_app(
        &self,
        body: &AdoptAppRequest,
    ) -> Result<AdoptedApp, CliError> {
        self.request_json(
            reqwest::Method::POST,
            "api/v1/app-provenance/adopt",
            Some(body),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBSERVER_PAGE: &str = r#"{"apps":[
        {"app_name":"TA-nix","installed_on":[
            {"host_id":"11111111-1111-4111-8111-111111111111","hostname":"idx1","observed_version":"9.0.0"},
            {"host_id":"22222222-2222-4222-8222-222222222222","hostname":"sh1"}],
         "missing_from":[],"total_enrolled":2,"coverage_pct":100.0,"stanza_count":4,
         "last_seen":"2026-10-29T08:00:00Z","observed_version":"9.0.1"}],
        "total_enrolled_hosts":2,"total":1,"limit":2000,"offset":0}"#;

    #[test]
    fn decodes_observer_apps_response() {
        let page: InsightsAppsPage = serde_json::from_str(OBSERVER_PAGE).expect("decode");
        assert_eq!(page.total, 1);
        assert_eq!(page.apps[0].installed_on.len(), 2);
    }

    #[test]
    fn flattens_and_filters_by_host() {
        let page: InsightsAppsPage = serde_json::from_str(OBSERVER_PAGE).unwrap();
        let all = flatten_installed(&page.apps, &|_| true);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].version.as_deref(), Some("9.0.0"));
        assert_eq!(all[1].version.as_deref(), Some("9.0.1"));
        let sh = flatten_installed(&page.apps, &|h| h.matches("SH1"));
        assert_eq!(sh.len(), 1);
        assert_eq!(sh[0].host_id, "22222222-2222-4222-8222-222222222222");
    }

    #[test]
    fn adopt_body_matches_observer_fields() {
        let body = AdoptAppRequest {
            scope: "apps".into(),
            app_name: "TA-nix".into(),
            host_id: "h".into(),
            inventory_group_id: None,
            management_mode: "managed".into(),
            content_source: "git_tree".into(),
            source_path: None,
        };
        let v = serde_json::to_value(&body).unwrap();
        let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "app_name",
                "content_source",
                "host_id",
                "management_mode",
                "scope"
            ]
        );
    }
}
