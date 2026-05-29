use super::{OidcError, OidcTokenProvider};

pub struct GitlabProvider;

#[async_trait::async_trait]
impl OidcTokenProvider for GitlabProvider {
    async fn fetch_token(&self, _audience: &str) -> Result<String, OidcError> {
        Err(OidcError::Other("not implemented".into()))
    }
}
