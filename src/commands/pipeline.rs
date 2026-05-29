use crate::ci::{self, CiPlatform, OidcError};
use crate::errors::CliError;
use crate::observer_client::Client;
use crate::resolver::ResolvedBackend;
use crate::Ctx;

pub struct AuthenticatedSession {
    pub platform: CiPlatform,
    pub backend: ResolvedBackend,
}

pub async fn authenticate(
    ctx: &Ctx,
    environment: Option<&str>,
    plan_id: Option<&str>,
) -> Result<(AuthenticatedSession, Client), CliError> {
    let platform = ci::detect_platform(ctx.ci_override);
    let jwt = ci::provider_for(platform)
        .fetch_token(ci::AUDIENCE)
        .await
        .map_err(map_oidc_error)?;
    let backend = crate::resolver::resolve(ctx, &jwt, platform, environment, plan_id).await?;
    let token =
        crate::oidc_exchange::exchange(&backend.observer_api_url, &jwt, platform, environment)
            .await?;
    let client = Client::new(backend.observer_api_url.clone(), token);
    Ok((AuthenticatedSession { platform, backend }, client))
}

pub fn map_oidc_error(err: OidcError) -> CliError {
    match err {
        OidcError::MissingEnv(msg) => CliError::Other(msg),
        OidcError::Http(msg) => CliError::Transport(msg),
        OidcError::Unsupported(msg) => CliError::UnsupportedPlatform(msg),
        OidcError::Other(msg) => CliError::Other(msg),
    }
}

pub fn map_cli_error(err: CliError) -> i32 {
    eprintln!("{err}");
    err.exit_code()
}
