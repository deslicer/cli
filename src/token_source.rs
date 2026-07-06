//! Bearer-token provisioning for Observer/proxy requests.
//!
//! Two strategies:
//! - `Static` — a pre-exchanged Observer API key (direct mode).
//! - `CiOidc` — the raw CI OIDC JWT, re-fetched from the CI platform's token
//!   service when it nears expiry or is rejected (proxy mode). CI OIDC JWTs
//!   are short-lived (minutes), so long-running commands like `change status`
//!   and `change deploy --wait` must not pin a single JWT for their lifetime.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use tokio::sync::Mutex;

use crate::ci::{self, CiPlatform};
use crate::errors::CliError;

/// Refresh the cached JWT this long before its `exp` claim.
const EXPIRY_SAFETY_MARGIN: Duration = Duration::from_secs(60);

pub struct TokenSource {
    kind: TokenSourceKind,
}

enum TokenSourceKind {
    Static(String),
    CiOidc {
        platform: CiPlatform,
        cache: Mutex<Option<CachedJwt>>,
    },
}

struct CachedJwt {
    token: String,
    /// `None` when the token payload could not be parsed — the token is then
    /// reused until the server rejects it (401-triggered refresh).
    refresh_after: Option<SystemTime>,
}

impl TokenSource {
    pub fn static_token(token: String) -> Self {
        Self {
            kind: TokenSourceKind::Static(token),
        }
    }

    /// CI OIDC source, optionally seeded with a JWT already fetched during
    /// backend resolution so the first request does not re-fetch.
    pub fn ci_oidc(platform: CiPlatform, initial_jwt: Option<String>) -> Self {
        let cache = initial_jwt.map(|token| CachedJwt {
            refresh_after: jwt_refresh_after(&token),
            token,
        });
        Self {
            kind: TokenSourceKind::CiOidc {
                platform,
                cache: Mutex::new(cache),
            },
        }
    }

    /// Current bearer value, refreshing an expiring CI OIDC JWT if needed.
    pub async fn bearer(&self) -> Result<String, CliError> {
        match &self.kind {
            TokenSourceKind::Static(token) => Ok(token.clone()),
            TokenSourceKind::CiOidc { platform, cache } => {
                let mut guard = cache.lock().await;
                if let Some(cached) = guard.as_ref() {
                    let expiring = cached
                        .refresh_after
                        .is_some_and(|deadline| SystemTime::now() >= deadline);
                    if !expiring {
                        return Ok(cached.token.clone());
                    }
                }
                let token = fetch_ci_jwt(*platform).await?;
                *guard = Some(CachedJwt {
                    refresh_after: jwt_refresh_after(&token),
                    token: token.clone(),
                });
                Ok(token)
            }
        }
    }

    /// Force a fresh token after a 401. Returns `None` for static tokens
    /// (nothing to refresh — the 401 is terminal).
    pub async fn force_refresh(&self) -> Result<Option<String>, CliError> {
        match &self.kind {
            TokenSourceKind::Static(_) => Ok(None),
            TokenSourceKind::CiOidc { platform, cache } => {
                let token = fetch_ci_jwt(*platform).await?;
                let mut guard = cache.lock().await;
                *guard = Some(CachedJwt {
                    refresh_after: jwt_refresh_after(&token),
                    token: token.clone(),
                });
                Ok(Some(token))
            }
        }
    }
}

impl From<String> for TokenSource {
    fn from(token: String) -> Self {
        Self::static_token(token)
    }
}

async fn fetch_ci_jwt(platform: CiPlatform) -> Result<String, CliError> {
    ci::provider_for(platform)
        .fetch_token(ci::AUDIENCE)
        .await
        .map_err(CliError::from)
}

/// Parse the JWT `exp` claim and subtract the safety margin.
fn jwt_refresh_after(jwt: &str) -> Option<SystemTime> {
    let payload_b64 = jwt.split('.').nth(1)?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    let exp = value.get("exp")?.as_u64()?;
    let expires_at = UNIX_EPOCH.checked_add(Duration::from_secs(exp))?;
    expires_at.checked_sub(EXPIRY_SAFETY_MARGIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_payload(json: serde_json::Value) -> String {
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.to_string().as_bytes());
        format!("header.{payload}.sig")
    }

    #[test]
    fn refresh_after_is_before_exp() {
        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 300;
        let jwt = encode_payload(serde_json::json!({ "exp": exp }));
        let refresh_after = jwt_refresh_after(&jwt).unwrap();
        let expires_at = UNIX_EPOCH + Duration::from_secs(exp);
        assert_eq!(refresh_after, expires_at - EXPIRY_SAFETY_MARGIN);
    }

    #[test]
    fn unparseable_token_has_no_refresh_deadline() {
        assert!(jwt_refresh_after("not-a-jwt").is_none());
        assert!(jwt_refresh_after("a.b.c").is_none());
    }

    #[tokio::test]
    async fn static_token_never_refreshes() {
        let source = TokenSource::static_token("dap_key".into());
        assert_eq!(source.bearer().await.unwrap(), "dap_key");
        assert!(source.force_refresh().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn seeded_ci_jwt_is_reused_until_expiry() {
        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;
        let jwt = encode_payload(serde_json::json!({ "exp": exp }));
        let source = TokenSource::ci_oidc(CiPlatform::Github, Some(jwt.clone()));
        assert_eq!(source.bearer().await.unwrap(), jwt);
    }
}
