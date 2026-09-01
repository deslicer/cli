use serde_json::Value;

use crate::cli::LogFormat;

pub fn print_output(format: LogFormat, json: &Value, human: &str) {
    match format {
        LogFormat::Json => println!("{}", pretty(json)),
        LogFormat::Human => print!("{human}"),
    }
}

pub fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

pub fn login_human(identity: &str, observer_api_url: &str, resolution_path: &str) -> String {
    format!(
        "Logged in\nIdentity: {identity}\nBackend: {observer_api_url}\nResolution: {resolution_path}\n"
    )
}

pub fn whoami_device_human(
    logged_in: bool,
    display_name: &str,
    tenant_id: &str,
    expires_at: &str,
    tenant_slug: Option<&str>,
) -> String {
    let workspace = workspace_label(tenant_id, tenant_slug);
    if logged_in {
        format!(
            "Logged in as {display_name}\nIdentity: device\nWorkspace: {workspace}\nExpires: {expires_at}\n"
        )
    } else {
        format!(
            "Session expired\nIdentity: device\nWorkspace: {workspace}\nExpires: {expires_at}\n"
        )
    }
}

pub fn whoami_token_human(observer_api_url: Option<&str>) -> String {
    match observer_api_url {
        Some(url) => format!("Logged in\nIdentity: observer_api_token\nBackend: {url}\n"),
        None => "Logged in\nIdentity: observer_api_token\n".to_string(),
    }
}

pub fn whoami_none_human() -> String {
    "Not logged in\nRun `deslicer auth login` and approve the code in the portal\n".to_string()
}

pub fn whoami_ci_human(platform: &str) -> String {
    format!("Logged in\nIdentity: ci\nPlatform: {platform}\n")
}

pub fn status_device_human(
    logged_in: bool,
    tenant_id: &str,
    display_name: &str,
    expires_at: &str,
    observer_api_url: &str,
    tenant_slug: Option<&str>,
) -> String {
    let state = if logged_in { "yes" } else { "no" };
    let workspace = workspace_label(tenant_id, tenant_slug);
    format!(
        "Identity: device\nLogged in: {state}\nWorkspace: {workspace}\nName: {display_name}\nExpires: {expires_at}\nBackend: {observer_api_url}\n"
    )
}

/// Prefer the portal slug. Never label a UUID as "Workspace" when a slug exists.
pub fn workspace_label<'a>(tenant_id: &'a str, tenant_slug: Option<&'a str>) -> &'a str {
    tenant_slug
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(tenant_id)
}

pub fn status_token_human(observer_api_url: Option<&str>) -> String {
    match observer_api_url {
        Some(url) => format!("Identity: observer_api_token\nBackend: {url}\n"),
        None => "Identity: observer_api_token\n".to_string(),
    }
}

pub fn status_ci_human(
    platform: &str,
    observer_api_url: Option<&str>,
    resolution_path: Option<&str>,
) -> String {
    let mut lines = vec!["Identity: ci".to_string(), format!("Platform: {platform}")];
    if let Some(url) = observer_api_url {
        lines.push(format!("Backend: {url}"));
    }
    if let Some(path) = resolution_path {
        lines.push(format!("Resolution: {path}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn login_human_omits_json_braces() {
        let text = login_human("device", "https://example.test", "device_session");
        assert!(text.contains("Logged in"));
        assert!(text.contains("Identity: device"));
        assert!(!text.contains('{'));
    }

    #[test]
    fn pretty_json_is_stable_object() {
        let value = json!({"logged_in": true, "identity": "device"});
        let text = pretty(&value);
        assert!(text.contains("\"logged_in\""));
        assert!(text.contains("true"));
    }

    #[test]
    fn whoami_human_uses_slug_not_uuid_when_present() {
        let text = whoami_device_human(
            true,
            "Ada",
            "2fb5ef22-12ad-4d20-9e0f-4736f47953bb",
            "2099-01-01T00:00:00.000Z",
            Some("dap-102"),
        );
        assert!(text.contains("Workspace: dap-102"));
        assert!(!text.contains("Workspace: 2fb5ef22"));
    }

    #[test]
    fn whoami_human_falls_back_to_tenant_id_without_slug() {
        let text = whoami_device_human(
            true,
            "Ada",
            "2fb5ef22-12ad-4d20-9e0f-4736f47953bb",
            "2099-01-01T00:00:00.000Z",
            None,
        );
        assert!(text.contains("Workspace: 2fb5ef22-12ad-4d20-9e0f-4736f47953bb"));
    }
}
