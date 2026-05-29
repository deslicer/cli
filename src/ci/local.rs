use super::{OidcError, OidcTokenProvider};

pub struct LocalProvider;

#[async_trait::async_trait]
impl OidcTokenProvider for LocalProvider {
    async fn fetch_token(&self, _audience: &str) -> Result<String, OidcError> {
        let token = std::env::var("DESLICER_DEV_TOKEN").map_err(|_| {
            OidcError::MissingEnv(
                "DESLICER_DEV_TOKEN (set DESLICER_DEV_TOKEN to a pre-issued local-dev token)"
                    .to_string(),
            )
        })?;
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

        std::env::set_var("DESLICER_DEV_TOKEN", "  local-dev-token  ");

        let token = LocalProvider
            .fetch_token("https://api.deslicer.ai")
            .await
            .unwrap();

        assert_eq!(token, "local-dev-token");

        std::env::remove_var("DESLICER_DEV_TOKEN");
    }

    #[tokio::test]
    async fn fetch_token_errors_when_env_missing() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::remove_var("DESLICER_DEV_TOKEN");

        let err = LocalProvider
            .fetch_token("https://api.deslicer.ai")
            .await
            .unwrap_err();

        match err {
            OidcError::MissingEnv(msg) => {
                assert!(msg.contains("DESLICER_DEV_TOKEN"));
                assert!(msg.contains("pre-issued local-dev token"));
            }
            other => panic!("expected MissingEnv, got {other:?}"),
        }
    }
}
