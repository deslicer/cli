//! Shared outbound HTTP client for every CLI hop (DAI, Observer, CI OIDC).
//!
//! REQ-SEC-004: rustls + TLS 1.3 minimum. Redirects are disabled so a 3xx to
//! http:// cannot bypass the scheme check. Plain HTTP is fail-closed except
//! loopback, or the dual-guard `DESLICER_ALLOW_HTTP` + `DESLICER_ENV`.

use std::time::Duration;

use url::Url;

use crate::errors::CliError;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

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
}
