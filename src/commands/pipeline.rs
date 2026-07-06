use crate::ci::{self, CiPlatform};
use crate::errors::CliError;
use crate::observer_client::Client;
use crate::resolver::ResolvedBackend;
use crate::token_source::TokenSource;
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
        .map_err(CliError::from)?;
    let backend = crate::resolver::resolve(ctx, &jwt, platform, environment, plan_id).await?;

    let client = if backend.proxy_mode {
        // Proxy mode authenticates every request with the CI OIDC JWT itself.
        // JWTs are short-lived, so the token source re-fetches on expiry/401.
        let tokens = TokenSource::ci_oidc(platform, Some(jwt));
        Client::new(backend.observer_api_url.clone(), tokens)
            .with_ci_platform(platform)
            .with_environment(environment.map(str::to_string))
    } else {
        let token =
            crate::oidc_exchange::exchange(&backend.observer_api_url, &jwt, platform, environment)
                .await?;
        Client::new(backend.observer_api_url.clone(), token)
    };

    Ok((AuthenticatedSession { platform, backend }, client))
}

pub fn map_cli_error(err: CliError) -> i32 {
    eprintln!("{err}");
    err.exit_code()
}

/// Standard rejection for commands that require the deslicer-ai CI proxy.
pub fn require_proxy_mode(session: &AuthenticatedSession, command: &str) -> Result<(), CliError> {
    if session.backend.proxy_mode {
        return Ok(());
    }
    Err(CliError::Other(format!(
        "`{command}` requires the deslicer-ai CI proxy (the Observer management \
         plane is not reachable from CI runners). Remove the --observer-api-url \
         override / OBSERVER_API_URL env var, or ask your platform admin to \
         enable CI proxy mode (CI_PROXY_MODE) on the deslicer-ai portal."
    )))
}
