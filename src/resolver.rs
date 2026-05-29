use crate::ci::CiPlatform;
use crate::errors::CliError;
use crate::Ctx;

#[derive(Debug, Clone)]
pub struct ResolvedBackend {
    pub observer_api_url: url::Url,
    pub audience: String,
    pub resolution_path: String,
}

pub async fn resolve(
    ctx: &Ctx,
    jwt: &str,
    platform: CiPlatform,
    environment: Option<&str>,
    plan_id: Option<&str>,
) -> Result<ResolvedBackend, CliError> {
    if let Some(url) = ctx.observer_api_url.clone() {
        return Ok(ResolvedBackend {
            observer_api_url: url,
            audience: crate::ci::AUDIENCE.to_string(),
            resolution_path: "observer_url_override".to_string(),
        });
    }
    let _ = (jwt, platform, environment, plan_id);
    Err(CliError::Other("resolver not implemented".to_string()))
}
