//! Remember which portal issued the device session.
//!
//! Login talks to `--deslicer-api-url`, but later commands default that flag
//! back to `https://api.deslicer.ai`. Agent runs live on the portal that
//! minted the session, so a show/enterprise login followed by a bare
//! `deslicer agent` would otherwise POST to the SaaS host and get an HTML 404.

use crate::token_store::StoredSession;
use crate::Ctx;

/// Portal origin stored on login, or derived from `observer_api_url`.
///
/// Existing sessions (pre-1.4) only have `observer_api_url`
/// (`https://ops.example/api/cli/observer/`). The origin of that URL is the
/// portal the CLI must call.
pub fn portal_url(session: &StoredSession) -> Option<url::Url> {
    if let Some(raw) = session.deslicer_api_url.as_deref() {
        if let Ok(url) = url::Url::parse(raw) {
            return Some(url);
        }
    }
    portal_origin_from_observer(&session.observer_api_url)
}

/// Uses the session portal unless the operator overrode `--deslicer-api-url`
/// or `DESLICER_API_URL`.
pub fn resolve_deslicer_api_url(ctx: &Ctx, session: &StoredSession) -> url::Url {
    resolve_deslicer_api_url_with(
        &ctx.deslicer_api_url,
        deslicer_api_url_is_explicit(),
        session,
    )
}

fn resolve_deslicer_api_url_with(
    requested: &url::Url,
    explicit: bool,
    session: &StoredSession,
) -> url::Url {
    if explicit {
        return requested.clone();
    }
    portal_url(session).unwrap_or_else(|| requested.clone())
}

pub fn deslicer_api_url_is_explicit() -> bool {
    std::env::var_os("DESLICER_API_URL").is_some()
        || std::env::args()
            .any(|arg| arg == "--deslicer-api-url" || arg.starts_with("--deslicer-api-url="))
}

fn portal_origin_from_observer(observer_api_url: &str) -> Option<url::Url> {
    let url = url::Url::parse(observer_api_url).ok()?;
    let origin = url.origin().ascii_serialization();
    url::Url::parse(&format!("{origin}/")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::LogFormat;
    use crate::token_store::StoredSession;

    fn session(observer: &str, portal: Option<&str>) -> StoredSession {
        StoredSession {
            cli_session_token: "dslcli_abc".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            tenant_id: "tenant".into(),
            display_name: "Ada".into(),
            observer_api_url: observer.into(),
            tenant_slug: None,
            deslicer_api_url: portal.map(str::to_string),
        }
    }

    fn ctx(url: &str) -> Ctx {
        Ctx {
            deslicer_api_url: url::Url::parse(url).unwrap(),
            observer_api_url: None,
            ci_override: None,
            log_format: LogFormat::Human,
        }
    }

    #[test]
    fn stored_portal_wins_over_observer_origin() {
        let session = session(
            "https://ops.deslicer.show/api/cli/observer/",
            Some("https://ops.deslicer.show/"),
        );
        assert_eq!(
            portal_url(&session).unwrap().as_str(),
            "https://ops.deslicer.show/"
        );
    }

    #[test]
    fn legacy_session_derives_portal_from_observer_url() {
        let session = session("https://ops.deslicer.show/api/cli/observer/", None);
        assert_eq!(
            portal_url(&session).unwrap().as_str(),
            "https://ops.deslicer.show/"
        );
    }

    #[test]
    fn resolve_uses_session_portal_when_the_flag_was_not_set() {
        let session = session("https://ops.deslicer.show/api/cli/observer/", None);
        let ctx = ctx("https://api.deslicer.ai/");
        assert_eq!(
            resolve_deslicer_api_url_with(&ctx.deslicer_api_url, false, &session).as_str(),
            "https://ops.deslicer.show/"
        );
    }

    #[test]
    fn explicit_flag_keeps_the_requested_host() {
        let session = session("https://ops.deslicer.show/api/cli/observer/", None);
        let ctx = ctx("https://api.deslicer.ai/");
        assert_eq!(
            resolve_deslicer_api_url_with(&ctx.deslicer_api_url, true, &session).as_str(),
            "https://api.deslicer.ai/"
        );
    }

    #[test]
    fn unparseable_observer_url_falls_back_to_ctx() {
        let session = session("not-a-url", None);
        let ctx = ctx("https://api.deslicer.ai/");
        assert_eq!(
            resolve_deslicer_api_url_with(&ctx.deslicer_api_url, false, &session).as_str(),
            "https://api.deslicer.ai/"
        );
    }
}
