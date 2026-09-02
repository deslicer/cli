use std::io::IsTerminal;

use url::Url;

use super::catalog::{DAP_PLATFORM_API_KEYS_PATH, DOCS_BASE_URL_ENV};

/// True when the CLI must not launch a GUI browser or block on user input.
pub fn is_non_interactive() -> bool {
    if std::env::var_os("CI").is_some_and(|value| !value.is_empty() && value != "0") {
        return true;
    }
    !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal()
}

/// Open a catalog URL in the default browser. The URL is built from our
/// topic table (+ optional `DESLICER_DOCS_BASE_URL` or the logged-in portal
/// origin), not from free-form operator paths (REQ-SEC-006).
pub fn open_url(url: &str, portal_base: Option<&Url>) -> Result<(), String> {
    if is_non_interactive() {
        return Ok(());
    }
    if !allowed_open_url(url, portal_base) {
        return Err("refusing to open a URL outside allowed docs hosts".into());
    }

    let status = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).status()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()
    } else {
        std::process::Command::new("xdg-open").arg(url).status()
    };

    match status {
        Ok(code) if code.success() => Ok(()),
        Ok(code) => Err(format!("browser helper exited {code}")),
        Err(err) => Err(format!("could not open browser: {err}")),
    }
}

pub fn allowed_open_url(url: &str, portal_base: Option<&Url>) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    if parsed.scheme() == "https" {
        if host == "github.com" && parsed.path().starts_with("/deslicer/") {
            return true;
        }
        if host == "docs.deslicer.io" {
            return true;
        }
        if configured_docs_host()
            .as_deref()
            .is_some_and(|configured| configured == host)
        {
            return true;
        }
    }
    portal_base.is_some_and(|portal| portal_origin_allows(&parsed, portal))
}

fn portal_origin_allows(url: &Url, portal: &Url) -> bool {
    if url.scheme() != portal.scheme() {
        return false;
    }
    if url.scheme() != "https" && url.scheme() != "http" {
        return false;
    }
    if url.host_str() != portal.host_str() {
        return false;
    }
    if url.port_or_known_default() != portal.port_or_known_default() {
        return false;
    }
    url.path() == DAP_PLATFORM_API_KEYS_PATH && url.query().is_none()
}

fn configured_docs_host() -> Option<String> {
    let raw = std::env::var(DOCS_BASE_URL_ENV).ok()?;
    let parsed = Url::parse(raw.trim()).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    parsed.host_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn portal() -> Url {
        Url::parse("https://ops.deslicer.show/").expect("url")
    }

    #[test]
    fn ci_env_forces_non_interactive() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::set_var("CI", "1");
        assert!(is_non_interactive());
        std::env::remove_var("CI");
    }

    #[test]
    fn rejects_non_deslicer_hosts() {
        assert!(!allowed_open_url("https://example.com/docs", None));
        assert!(!allowed_open_url(
            "https://evil.example/https://docs.deslicer.io/",
            None
        ));
        assert!(!allowed_open_url("http://docs.deslicer.io/cli", None));
        assert!(allowed_open_url(
            "https://github.com/deslicer/cli/blob/main/docs/quickstart.md",
            None
        ));
        assert!(allowed_open_url(
            "https://docs.deslicer.io/cli/quickstart",
            None
        ));
    }

    #[test]
    fn allows_logged_in_portal_api_keys_and_rejects_evil() {
        let api_keys = "https://ops.deslicer.show/dashboard/dap/api-keys";
        assert!(allowed_open_url(api_keys, Some(&portal())));
        assert!(!allowed_open_url(
            "https://evil.com/dashboard/dap/api-keys",
            Some(&portal())
        ));
        assert!(!allowed_open_url(
            "https://ops.deslicer.show.evil.com/dashboard/dap/api-keys",
            Some(&portal())
        ));
        assert!(!allowed_open_url(api_keys, None));
        assert!(!allowed_open_url(
            "https://ops.deslicer.show/dashboard/dap/api-keys?create=1",
            Some(&portal())
        ));
    }
}
