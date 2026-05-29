use super::{OidcError, OidcTokenProvider};

pub struct BitbucketProvider;

#[async_trait::async_trait]
impl OidcTokenProvider for BitbucketProvider {
    async fn fetch_token(&self, _audience: &str) -> Result<String, OidcError> {
        let token = std::env::var("BITBUCKET_STEP_OIDC_TOKEN")
            .map_err(|_| OidcError::MissingEnv("BITBUCKET_STEP_OIDC_TOKEN".to_string()))?;
        Ok(token.trim().to_string())
    }
}

#[cfg(test)]
// ENV_LOCK only serializes env access across single-threaded #[tokio::test] cases;
// holding it across the await is safe (no cross-task contention).
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn fetch_token_returns_trimmed_env_value() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::set_var("BITBUCKET_STEP_OIDC_TOKEN", "  bitbucket-jwt-token  ");

        let token = BitbucketProvider
            .fetch_token("https://api.deslicer.ai")
            .await
            .unwrap();

        assert_eq!(token, "bitbucket-jwt-token");

        std::env::remove_var("BITBUCKET_STEP_OIDC_TOKEN");
    }

    #[tokio::test]
    async fn fetch_token_errors_when_env_missing() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::remove_var("BITBUCKET_STEP_OIDC_TOKEN");

        let err = BitbucketProvider
            .fetch_token("https://api.deslicer.ai")
            .await
            .unwrap_err();

        assert!(matches!(err, OidcError::MissingEnv(_)));
    }
}
