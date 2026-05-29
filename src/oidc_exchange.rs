use crate::ci::CiPlatform;
use crate::errors::CliError;

pub async fn exchange(
    observer_api_url: &url::Url,
    jwt: &str,
    platform: CiPlatform,
    environment: Option<&str>,
) -> Result<String, CliError> {
    let _ = (observer_api_url, jwt, platform, environment);
    Err(CliError::Other("oidc exchange not implemented".to_string()))
}
