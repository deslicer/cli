use super::{OidcError, OidcTokenProvider};

pub struct GithubProvider;

#[async_trait::async_trait]
impl OidcTokenProvider for GithubProvider {
    async fn fetch_token(&self, _audience: &str) -> Result<String, OidcError> {
        Err(OidcError::Other("not implemented".into()))
    }
}
