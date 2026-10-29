//! Registry-sourced change plans via app provenance.

use serde::Serialize;

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
    let valid = |seg: &str| {
        seg.len() <= 128
            && seg != "."
            && seg != ".."
            && seg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
    };
    if !valid(app) || !valid(version) {
        return Err(CliError::InvalidInput(format!(
            "invalid --from-registry {spec:?}; app and version may use letters, digits, '.', '_', '-', '+'"
        )));
    }
    Ok(RegistryAppRef {
        app: app.to_string(),
        version: version.to_string(),
    })
}

/// Observer registry ref (`splunkbase:<app>/<ver>`) for `source_reference`.
pub fn registry_source_reference(app_ref: &RegistryAppRef) -> String {
    format!("splunkbase:{}/{}", app_ref.app, app_ref.version)
}

impl Client {
    /// Create a draft registry-sourced plan (`POST /api/v1/plans` with
    /// `source_type = "registry"`), DAP #1395.
    ///
    /// Observer resolves the release from the target group's durable pin and
    /// accepts it under every change-management mode. The plan stays in draft
    /// until the compile-runner posts content; it never auto-approves.
    pub async fn create_plan_from_registry(
        &self,
        app_ref: &RegistryAppRef,
        target_group_id: &str,
        name: Option<&str>,
    ) -> Result<ChangePlan, CliError> {
        #[derive(Serialize)]
        struct Body<'a> {
            source_type: &'a str,
            source_reference: String,
            target_group_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
        }

        let body = Body {
            source_type: "registry",
            source_reference: registry_source_reference(app_ref),
            target_group_id,
            name,
        };
        let bytes = self
            .request_bytes(reqwest::Method::POST, "api/v1/plans", Some(&body))
            .await?;
        parse_registry_plan_response(&bytes)
    }
}

fn parse_registry_plan_response(bytes: &[u8]) -> Result<ChangePlan, CliError> {
    let envelope: ChangePlanResponse = serde_json::from_slice(bytes)
        .map_err(|e| CliError::Transport(format!("invalid plan create response: {e}")))?;
    match envelope.plan {
        Some(plan) if !plan.id.is_empty() => Ok(plan),
        _ => Err(CliError::Other(format!(
            "registry plan creation failed: {}",
            envelope.message
        ))),
    }
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
        assert!(parse_registry_app_ref("a/b@1.0").is_err());
        assert!(parse_registry_app_ref("..@1.0").is_err());
    }

    #[test]
    fn builds_splunkbase_source_reference() {
        let r = parse_registry_app_ref("Splunk_SA_CIM@6.1.0").unwrap();
        assert_eq!(
            registry_source_reference(&r),
            "splunkbase:Splunk_SA_CIM/6.1.0"
        );
    }

    #[test]
    fn parses_envelope_plan() {
        let json = br#"{"success":true,"message":"ok","plan":{"id":"p1","status":"draft"}}"#;
        let plan = parse_registry_plan_response(json).unwrap();
        assert_eq!(plan.id, "p1");
        assert_eq!(plan.status, "draft");
    }
}
