//! Shared outbound HTTP client for every CLI hop (DAI, Observer, CI OIDC).
//!
//! REQ-SEC-004: rustls + TLS 1.3 minimum. Redirects are disabled so a 3xx to
//! http:// cannot bypass the scheme check. Plain HTTP is fail-closed except
//! loopback, or the dual-guard `DESLICER_ALLOW_HTTP` + `DESLICER_ENV`.

use std::time::Duration;

use url::Url;

use crate::errors::CliError;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Silence tolerated on a streaming body before it is treated as dead.
///
/// The server writes an SSE keepalive comment every 15s, so anything past a
/// few multiples of that means the connection is gone rather than slow.
const STREAM_READ_TIMEOUT: Duration = Duration::from_secs(90);
const STREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const STREAM_TCP_KEEPALIVE: Duration = Duration::from_secs(30);

const LOCAL_ENVS: &[&str] = &["dev", "test", "local"];

fn base_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(concat!("deslicer-cli/", env!("CARGO_PKG_VERSION")))
        .use_rustls_tls()
        .min_tls_version(reqwest::tls::Version::TLS_1_3)
        .redirect(reqwest::redirect::Policy::none())
}

pub fn try_client() -> Result<reqwest::Client, CliError> {
    base_builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| CliError::Transport(format!("build HTTP client: {e}")))
}

pub fn client() -> reqwest::Client {
    // rustls-tls is compiled in; builder failure means a broken TLS backend.
    #[allow(clippy::expect_used)]
    try_client().expect("reqwest rustls client builder is infallible with rustls-tls")
}

/// Client for long-lived streaming responses (agent runs).
///
/// Deliberately has no total request timeout: an agent run legitimately takes
/// minutes, and a deadline that fires mid-stream would look identical to the
/// agent failing. Liveness comes from `read_timeout`, which resets on every
/// chunk, so a stalled connection is still caught — just not a slow one.
pub fn try_streaming_client() -> Result<reqwest::Client, CliError> {
    base_builder()
        .connect_timeout(STREAM_CONNECT_TIMEOUT)
        .read_timeout(STREAM_READ_TIMEOUT)
        .tcp_keepalive(STREAM_TCP_KEEPALIVE)
        .build()
        .map_err(|e| CliError::Transport(format!("build streaming HTTP client: {e}")))
}

pub fn assert_url_allowed(url: &Url) -> Result<(), CliError> {
    match url.scheme() {
        "https" => Ok(()),
        "http" if is_loopback_host(url) || allow_insecure_http() => Ok(()),
        "http" => Err(CliError::Transport(
            "plain HTTP is only allowed for localhost, or when \
             DESLICER_ALLOW_HTTP=true and DESLICER_ENV is dev, test, or local"
                .into(),
        )),
        other => Err(CliError::Transport(format!(
            "unsupported URL scheme `{other}` (use https)"
        ))),
    }
}

/// Join an API-relative path onto a configured base URL.
///
/// `Url::join` follows RFC 3986 and replaces the final path segment when the
/// base has no trailing slash (`…/observer` + `api/v1/groups` → `…/api/v1/groups`).
/// Observer and DAI bases are directories (direct root or `/api/cli/observer`), so
/// both `…/observer` and `…/observer/` must preserve the prefix.
pub fn join_api_path(base: &Url, path: &str) -> Result<Url, CliError> {
    let base = ensure_directory_base(base);
    let path = path.trim_start_matches('/');
    base.join(path)
        .map_err(|e| CliError::Transport(format!("invalid URL join: {e}")))
}

fn ensure_directory_base(base: &Url) -> Url {
    if base.path().ends_with('/') {
        return base.clone();
    }
    let mut with_slash = base.clone();
    let path = format!("{}/", with_slash.path());
    with_slash.set_path(&path);
    with_slash
}

const MAX_RAW_ERROR_BODY_CHARS: usize = 200;

/// Operator-facing summary of an HTTP error body.
///
/// JSON payloads keep their `detail` / `message` / `error` field. HTML and other
/// non-JSON bodies are truncated so a portal 404 page does not flood stderr or
/// `--log-format json` output.
pub fn http_error_message(body: &str, status: reqwest::StatusCode) -> String {
    http_error_message_with_path(body, status, None)
}

/// Like [`http_error_message`], optionally including the request path (no host).
pub fn http_error_message_with_path(
    body: &str,
    status: reqwest::StatusCode,
    request_path: Option<&str>,
) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        for key in ["detail", "message", "error"] {
            if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    return text.to_string();
                }
            }
        }
    }

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return format!("HTTP {status}");
    }

    let mut parts = vec![format!("HTTP {status}")];
    if looks_like_html(trimmed) {
        parts.push("content-type text/html".into());
        if let Some(path) = request_path.map(str::trim).filter(|p| !p.is_empty()) {
            parts.push(format!("path {path}"));
        }
        // Drop the HTML body entirely — even a 200-char prefix is noisy markup.
        parts
            .push("Is OBSERVER_API_URL (or DESLICER_API_URL) pointing at the Observer API?".into());
        return parts.join("; ");
    }

    if let Some(path) = request_path.map(str::trim).filter(|p| !p.is_empty()) {
        parts.push(format!("path {path}"));
    }
    let snippet = truncate_chars(trimmed, MAX_RAW_ERROR_BODY_CHARS);
    format!("{}: {snippet}", parts.join("; "))
}

fn looks_like_html(body: &str) -> bool {
    let lowered = body.to_ascii_lowercase();
    let trimmed = lowered.trim_start();
    trimmed.starts_with('<') || trimmed.contains("<!doctype") || trimmed.contains("<html")
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut truncated = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        truncated.push('…');
    }
    // Collapse runs of whitespace so a minified HTML page stays readable.
    truncated.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_loopback_host(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| {
            let trimmed = v.trim().to_ascii_lowercase();
            trimmed == "true" || trimmed == "1"
        })
        .unwrap_or(false)
}

fn env_is_local(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| LOCAL_ENVS.contains(&v.trim().to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn allow_insecure_http() -> bool {
    let flag = env_flag_enabled("DESLICER_ALLOW_HTTP") || env_flag_enabled("INSECURE_HTTP");
    let local = env_is_local("DESLICER_ENV") || env_is_local("DAP_ENV");
    flag && local
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn parse(raw: &str) -> Url {
        Url::parse(raw).expect("url")
    }

    fn clear_http_guards() {
        for name in [
            "DESLICER_ALLOW_HTTP",
            "INSECURE_HTTP",
            "DESLICER_ENV",
            "DAP_ENV",
        ] {
            std::env::remove_var(name);
        }
    }

    #[test]
    fn https_is_always_allowed() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_http_guards();
        assert!(assert_url_allowed(&parse("https://api.deslicer.ai/health")).is_ok());
    }

    #[test]
    fn loopback_http_is_allowed() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_http_guards();
        assert!(assert_url_allowed(&parse("http://127.0.0.1:8080/api/v1/plans")).is_ok());
        assert!(assert_url_allowed(&parse("http://localhost:8080/health")).is_ok());
    }

    #[test]
    fn remote_http_is_denied_without_dual_guard() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_http_guards();
        let err = assert_url_allowed(&parse("http://observer.example.com:8080/")).unwrap_err();
        assert!(err.to_string().contains("plain HTTP"));
    }

    #[test]
    fn remote_http_requires_both_guards() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_http_guards();
        std::env::set_var("DESLICER_ALLOW_HTTP", "true");
        let denied = assert_url_allowed(&parse("http://observer.example.com/"));
        assert!(denied.is_err());
        std::env::set_var("DESLICER_ENV", "local");
        assert!(assert_url_allowed(&parse("http://observer.example.com/")).is_ok());
        clear_http_guards();
    }

    #[test]
    fn ftp_is_rejected() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_http_guards();
        assert!(assert_url_allowed(&parse("ftp://example.com/x")).is_err());
    }

    #[test]
    fn request_timeout_allows_cold_create_window() {
        assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(120));
    }

    #[test]
    fn join_api_path_keeps_prefix_without_trailing_slash() {
        let base = parse("https://staging-api.deslicer.ai/api/cli/observer");
        let joined = join_api_path(&base, "api/v1/groups").expect("join");
        assert_eq!(
            joined.as_str(),
            "https://staging-api.deslicer.ai/api/cli/observer/api/v1/groups"
        );
    }

    #[test]
    fn join_api_path_keeps_prefix_with_trailing_slash() {
        let base = parse("https://staging-api.deslicer.ai/api/cli/observer/");
        let joined = join_api_path(&base, "api/v1/groups").expect("join");
        assert_eq!(
            joined.as_str(),
            "https://staging-api.deslicer.ai/api/cli/observer/api/v1/groups"
        );
    }

    #[test]
    fn join_api_path_strips_leading_slash_on_relative_path() {
        let base = parse("https://api.deslicer.ai/api/cli/observer");
        let joined = join_api_path(&base, "/api/v1/plans/abc").expect("join");
        assert_eq!(
            joined.as_str(),
            "https://api.deslicer.ai/api/cli/observer/api/v1/plans/abc"
        );
    }

    #[test]
    fn join_api_path_root_base_without_trailing_slash() {
        let base = parse("https://observer.example.test");
        let joined = join_api_path(&base, "api/v1/me").expect("join");
        assert_eq!(joined.as_str(), "https://observer.example.test/api/v1/me");
    }

    #[test]
    fn http_error_message_keeps_json_fields() {
        let msg = http_error_message(
            r#"{"error":"not_found","message":"Plan abc not found"}"#,
            reqwest::StatusCode::NOT_FOUND,
        );
        assert_eq!(msg, "Plan abc not found");
    }

    #[test]
    fn http_error_message_drops_html_404_body() {
        let html = format!(
            "<!DOCTYPE html><html lang=\"en\" data-scroll-behavior=\"smooth\"><body>{}</body></html>",
            "x".repeat(4_000)
        );
        let msg = http_error_message_with_path(
            &html,
            reqwest::StatusCode::NOT_FOUND,
            Some("api/v1/groups"),
        );
        assert!(msg.contains("HTTP 404 Not Found"));
        assert!(msg.contains("content-type text/html"));
        assert!(msg.contains("path api/v1/groups"));
        assert!(msg.contains("OBSERVER_API_URL"));
        assert!(!msg.contains("data-scroll-behavior"));
        assert!(!msg.contains("<!DOCTYPE"));
        assert!(msg.len() < 250);
    }

    #[test]
    fn http_error_message_drops_html_without_path() {
        // oidc_exchange / resolver call http_error_message without a path.
        let html = "<!DOCTYPE html><html><body>not found</body></html>";
        let msg = http_error_message(html, reqwest::StatusCode::NOT_FOUND);
        assert!(msg.contains("HTTP 404 Not Found"));
        assert!(msg.contains("content-type text/html"));
        assert!(msg.contains("OBSERVER_API_URL"));
        assert!(!msg.contains("<!DOCTYPE"));
        assert!(!msg.contains("<html"));
    }

    #[test]
    fn http_error_message_truncates_plain_non_json_body() {
        let body = "plain ".repeat(80);
        let msg = http_error_message(&body, reqwest::StatusCode::BAD_GATEWAY);
        assert!(msg.contains("HTTP 502 Bad Gateway"));
        assert!(msg.contains('…'));
        assert!(msg.len() < 280);
    }
}
