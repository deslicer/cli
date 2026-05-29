use super::{OidcError, OidcTokenProvider};

pub struct GitlabProvider;

#[async_trait::async_trait]
impl OidcTokenProvider for GitlabProvider {
    async fn fetch_token(&self, _audience: &str) -> Result<String, OidcError> {
        let token = std::env::var("DESLICER_OIDC_TOKEN")
            .map_err(|_| OidcError::MissingEnv("DESLICER_OIDC_TOKEN".to_string()))?;
        Ok(token.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn fetch_token_returns_trimmed_env_value() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::set_var("DESLICER_OIDC_TOKEN", "  gitlab-jwt-token  ");

        let token = GitlabProvider
            .fetch_token("https://api.deslicer.ai")
            .await
            .unwrap();

        assert_eq!(token, "gitlab-jwt-token");

        std::env::remove_var("DESLICER_OIDC_TOKEN");
    }

    #[tokio::test]
    async fn fetch_token_errors_when_env_missing() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::remove_var("DESLICER_OIDC_TOKEN");

        let err = GitlabProvider
            .fetch_token("https://api.deslicer.ai")
            .await
            .unwrap_err();

        assert!(matches!(err, OidcError::MissingEnv(_)));
    }
}
