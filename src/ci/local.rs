use super::{OidcError, OidcTokenProvider};

pub struct LocalProvider;

#[async_trait::async_trait]
impl OidcTokenProvider for LocalProvider {
    async fn fetch_token(&self, _audience: &str) -> Result<String, OidcError> {
        Err(OidcError::Other("not implemented".into()))
    }
}
