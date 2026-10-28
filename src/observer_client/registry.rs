//! Registry-sourced change plans via app provenance.

use serde::{Deserialize, Serialize};

use super::types::{ChangePlan, ChangePlanResponse};
use super::Client;
use crate::errors::CliError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryAppRef {
    pub app: String,
    pub version: String,
}

/// Parse `<app>@<ver>` for `--from-registry`.
pub fn parse_registry_app_ref(spec: &str) -> Result<RegistryAppRef, CliError> {
    let spec = spec.trim();
    let Some((app, version)) = spec.rsplit_once('@') else {
        return Err(CliError::InvalidInput(format!(
            "invalid --from-registry {spec:?}; expected <app>@<ver>"
        )));
    };
    let app = app.trim();
    let version = version.trim();
    if app.is_empty() || version.is_empty() {
        return Err(CliError::InvalidInput(
            "--from-registry requires non-empty <app> and <ver> (e.g. myapp@1.2.3)".into(),
        ));
    }
    if app.contains('@') {
        return Err(CliError::InvalidInput(
            "--from-registry app name must not contain '@'".into(),
        ));
    }
    Ok(RegistryAppRef {
        app: app.to_string(),
        version: version.to_string(),
    })
}

impl Client {
    /// Create a Pending plan by comparing a registry app version to deployed state.
    ///
    /// Works in any change-management mode. Never auto-approves.
    pub async fn create_plan_from_registry(
        &self,
        app: &str,
        version: &str,
        target_group_id: Option<&str>,
        name: Option<&str>,
    ) -> Result<ChangePlan, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            app: &'a str,
            version: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            target_group_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
            /// Request a draft plan row (Pending), not an auto-apply.
            create_plan: bool,
        }

        let body = Body {
            app,
            version,
            target_group_id,
            name,
            create_plan: true,
        };
        let bytes = self
            .request_bytes(
                reqwest::Method::POST,
                "api/v1/app-provenance/compare-registry",
                Some(&body),
            )
            .await?;
        parse_registry_plan_response(&bytes)
    }
}

fn parse_registry_plan_response(bytes: &[u8]) -> Result<ChangePlan, CliError> {
    if let Ok(envelope) = serde_json::from_slice::<ChangePlanResponse>(bytes) {
        if let Some(plan) = envelope.plan {
            if envelope.success || !plan.id.is_empty() {
                return Ok(plan);
            }
        }
        if !envelope.success && !envelope.message.is_empty() {
            return Err(CliError::Other(envelope.message));
        }
    }
    if let Ok(plan) = serde_json::from_slice::<ChangePlan>(bytes) {
        if !plan.id.is_empty() {
            return Ok(plan);
        }
    }
    #[derive(Deserialize)]
    struct Nested {
        #[serde(default)]
        plan: Option<ChangePlan>,
        #[serde(default)]
        draft: Option<ChangePlan>,
        #[serde(default)]
        change_plan: Option<ChangePlan>,
    }
    if let Ok(nested) = serde_json::from_slice::<Nested>(bytes) {
        if let Some(plan) = nested
            .plan
            .or(nested.draft)
            .or(nested.change_plan)
            .filter(|p| !p.id.is_empty())
        {
            return Ok(plan);
        }
    }
    Err(CliError::Transport(
        "invalid compare-registry response: expected a change plan".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_app_at_version() {
        let r = parse_registry_app_ref("search@1.2.3").unwrap();
        assert_eq!(r.app, "search");
        assert_eq!(r.version, "1.2.3");
        let r = parse_registry_app_ref("  my-app@2.0.0-rc1  ").unwrap();
        assert_eq!(r.app, "my-app");
        assert_eq!(r.version, "2.0.0-rc1");
    }

    #[test]
    fn rejects_bad_specs() {
        assert!(parse_registry_app_ref("noseparator").is_err());
        assert!(parse_registry_app_ref("@1.0").is_err());
        assert!(parse_registry_app_ref("app@").is_err());
        assert!(parse_registry_app_ref("a@b@c").is_err());
    }

    #[test]
    fn parses_envelope_plan() {
        let json =
            br#"{"success":true,"message":"ok","plan":{"id":"p1","status":"pending_approval"}}"#;
        let plan = parse_registry_plan_response(json).unwrap();
        assert_eq!(plan.id, "p1");
        assert_eq!(plan.status, "pending_approval");
    }
}
