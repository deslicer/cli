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
        &ctx.deslicer_api_url,          // pragma: allowlist secret
        deslicer_api_url_is_explicit(), // pragma: allowlist secret
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

/// Refuse to use a device credential against a different API origin (issue #268).
///
/// Returns before any authenticated request is built. Matching origins return Ok.
pub fn reject_incompatible_session(
    ctx: &Ctx,
    session: &StoredSession,
) -> Result<(), crate::errors::CliError> {
    let Some(detail) = incompatible_api_override(ctx, session) else {
        return Ok(());
    };
    Err(crate::errors::CliError::SessionOriginMismatch(format!(
        "{detail}. Refusing to send the device credential. Re-run auth login against the intended API, drop the override, or set DESLICER_IGNORE_DEVICE_SESSION=1 to skip the stored session without logging it out. Empty HOME does not isolate the kernel keyring." // pragma: allowlist secret
    )))
}

pub fn incompatible_api_override(ctx: &Ctx, session: &StoredSession) -> Option<String> {
    describe_origin_mismatch(
        &ctx.deslicer_api_url,          // pragma: allowlist secret
        deslicer_api_url_is_explicit(), // pragma: allowlist secret
        ctx.observer_api_url.as_ref(),
        session,
    )
}

pub(crate) fn describe_origin_mismatch(
    requested_portal: &url::Url,
    portal_explicit: bool,
    requested_observer: Option<&url::Url>,
    session: &StoredSession,
) -> Option<String> {
    let mut problems = Vec::new();
    if portal_explicit {
        if let Some(portal) = portal_url(session) {
            if !same_origin(requested_portal, &portal) {
                problems.push(format!(
                    "device session was issued for {} but the requested API URL is {}",
                    origin_display(&portal),
                    origin_display(requested_portal)
                ));
            }
        }
    }
    if let Some(observer) = requested_observer {
        if let Ok(session_observer) = url::Url::parse(&session.observer_api_url) {
            if !same_origin(observer, &session_observer) {
                problems.push(format!(
                    "device session observer is {} but the requested observer URL is {}",
                    origin_display(&session_observer),
                    origin_display(observer)
                ));
            }
        }
    }
    if problems.is_empty() {
        None
    } else {
        Some(problems.join("; "))
    }
}

fn same_origin(left: &url::Url, right: &url::Url) -> bool {
    left.scheme() == right.scheme()
        && host_key(left) == host_key(right)
        && left.port_or_known_default() == right.port_or_known_default()
}

fn host_key(url: &url::Url) -> Option<String> {
    Some(url.host_str()?.to_ascii_lowercase())
}

fn origin_display(url: &url::Url) -> String {
    let host = url.host_str().unwrap_or("");
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
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

    #[test]
    fn matching_origin_is_accepted_and_sends_nothing() {
        let session = session(
            "https://msp.example/api/cli/observer/",
            Some("https://msp.example/"),
        );
        let requested = url::Url::parse("https://msp.example/").unwrap();
        // A match returns before any HTTP client exists.
        assert!(describe_origin_mismatch(&requested, true, None, &session).is_none());
    }

    #[test]
    fn mismatched_portal_is_rejected_before_a_request() {
        let session = session(
            "https://msp.example/api/cli/observer/",
            Some("https://msp.example/"),
        );
        let requested = url::Url::parse("https://staging.example/").unwrap();
        let detail = describe_origin_mismatch(&requested, true, None, &session).expect("mismatch");
        assert!(detail.contains("msp.example"));
        assert!(detail.contains("staging.example"));
        let err = crate::errors::CliError::SessionOriginMismatch(detail);
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn mismatched_observer_is_rejected() {
        let session = session(
            "https://msp.example/api/cli/observer/",
            Some("https://msp.example/"),
        );
        let portal = url::Url::parse("https://msp.example/").unwrap();
        let observer = url::Url::parse("https://other-observer.example/").unwrap();
        assert!(describe_origin_mismatch(&portal, false, Some(&observer), &session).is_some());
    }
}
