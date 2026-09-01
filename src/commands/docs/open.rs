use url::Url;

use super::catalog::DOCS_BASE_URL_ENV;

/// Open a catalog URL in the default browser. The URL is built from our
/// topic table (+ optional `DESLICER_DOCS_BASE_URL`), not from free-form
/// operator paths (REQ-SEC-006).
pub fn open_url(url: &str) -> Result<(), String> {
    if !allowed_open_url(url) {
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

fn allowed_open_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    if host == "github.com" && parsed.path().starts_with("/deslicer/") {
        return true;
    }
    if host == "docs.deslicer.io" {
        return true;
    }
    configured_docs_host()
        .as_deref()
        .is_some_and(|configured| configured == host)
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

    #[test]
    fn rejects_non_deslicer_hosts() {
        assert!(!allowed_open_url("https://example.com/docs"));
        assert!(!allowed_open_url(
            "https://evil.example/https://docs.deslicer.io/"
        ));
        assert!(!allowed_open_url("http://docs.deslicer.io/cli"));
        assert!(allowed_open_url(
            "https://github.com/deslicer/cli/blob/main/docs/quickstart.md"
        ));
        assert!(allowed_open_url("https://docs.deslicer.io/cli/quickstart"));
    }
}
