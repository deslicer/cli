//! Static Observer API key (`DESLICER_API_TOKEN`) for direct management-plane
//! access. Env-only so the secret never appears in process argv (REQ-LOG-007).

use crate::cli::Ctx;

/// Resolution path stamped on sessions that skip DAI / OIDC.
pub const RESOLUTION_PATH: &str = "observer_api_token";

pub fn api_token() -> Option<String> {
    std::env::var("DESLICER_API_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// Both `OBSERVER_API_URL` and `DESLICER_API_TOKEN` are set — talk to Observer
/// directly and skip CI OIDC / device login.
pub fn direct_auth_ready(ctx: &Ctx) -> bool {
    ctx.observer_api_url.is_some() && api_token().is_some()
}

#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ci::CiPlatform;
    use crate::cli::LogFormat;
    use url::Url;

    fn ctx_with_observer(url: Option<&str>) -> Ctx {
        Ctx {
            deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
            observer_api_url: url.map(|u| Url::parse(u).expect("observer url")),
            ci_override: Some(CiPlatform::Github),
            log_format: LogFormat::Human,
        }
    }

    #[test]
    fn token_is_none_when_unset_or_blank() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::remove_var("DESLICER_API_TOKEN");
        assert!(api_token().is_none());
        std::env::set_var("DESLICER_API_TOKEN", "   ");
        assert!(api_token().is_none());
        std::env::remove_var("DESLICER_API_TOKEN");
    }

    #[test]
    fn direct_auth_requires_url_and_token() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::remove_var("DESLICER_API_TOKEN");
        assert!(!direct_auth_ready(&ctx_with_observer(Some(
            "https://observer.example.test"
        ))));
        std::env::set_var("DESLICER_API_TOKEN", "dap_tools_key");
        assert!(!direct_auth_ready(&ctx_with_observer(None)));
        assert!(direct_auth_ready(&ctx_with_observer(Some(
            "https://observer.example.test"
        ))));
        std::env::remove_var("DESLICER_API_TOKEN");
    }
}
