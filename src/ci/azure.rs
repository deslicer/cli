use super::{OidcError, OidcTokenProvider};

pub struct AzureProvider;

#[async_trait::async_trait]
impl OidcTokenProvider for AzureProvider {
    async fn fetch_token(&self, _audience: &str) -> Result<String, OidcError> {
        Err(OidcError::Other("not implemented".into()))
    }
}
