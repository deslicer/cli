use super::{OidcError, OidcTokenProvider};

pub struct LocalProvider;

#[async_trait::async_trait]
impl OidcTokenProvider for LocalProvider {
    async fn fetch_token(&self, _audience: &str) -> Result<String, OidcError> {
        Err(OidcError::Unsupported(
            "local OIDC is not supported; use device login interactively or \
             OBSERVER_API_URL with DESLICER_API_TOKEN for automation"
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_provider_never_returns_a_token() {
        let err = LocalProvider
            .fetch_token("https://api.deslicer.ai")
            .await
            .unwrap_err();

        assert!(matches!(err, OidcError::Unsupported(_)));
    }
}
