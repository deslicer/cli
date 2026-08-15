use crate::ci::{self, CiPlatform, AUDIENCE};
use crate::errors::CliError;
use crate::observer_client::Client;
use crate::observer_token;
use crate::resolver::ResolvedBackend;
use crate::token_source::TokenSource;
use crate::token_store::{load_active_session, StoredSession};
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
    if let Some(pair) = client_from_observer_token(ctx, platform, environment)? {
        return Ok(pair);
    }
    if platform == CiPlatform::Local {
        if let Some(session) = load_active_session()? {
            return client_from_device_session(session);
        }
    }
    let jwt = ci::provider_for(platform)
        .fetch_token(ci::AUDIENCE)
        .await
        .map_err(|err| match err {
            crate::ci::OidcError::MissingEnv(_) if platform == CiPlatform::Local => {
                CliError::Other(
                    "not logged in. Run `deslicer auth login` and approve the code in the portal"
                        .into(),
                )
            }
            other => CliError::from(other),
        })?;
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

impl AuthenticatedSession {
    pub fn is_device_session(&self) -> bool {
        self.backend.resolution_path == "device_session"
    }

    pub fn is_observer_api_token(&self) -> bool {
        self.backend.resolution_path == observer_token::RESOLUTION_PATH
    }
}

fn client_from_observer_token(
    ctx: &Ctx,
    platform: CiPlatform,
    environment: Option<&str>,
) -> Result<Option<(AuthenticatedSession, Client)>, CliError> {
    let Some(token) = observer_token::api_token() else {
        return Ok(None);
    };
    let Some(observer_api_url) = ctx.observer_api_url.clone() else {
        return Ok(None);
    };
    crate::http::assert_url_allowed(&observer_api_url)?;
    let backend = ResolvedBackend {
        observer_api_url: observer_api_url.clone(),
        audience: AUDIENCE.to_string(),
        resolution_path: observer_token::RESOLUTION_PATH.to_string(),
        proxy_mode: false,
    };
    let client = Client::new(observer_api_url, TokenSource::static_token(token))
        .with_ci_platform(platform)
        .with_environment(environment.map(str::to_string));
    Ok(Some((AuthenticatedSession { platform, backend }, client)))
}

fn client_from_device_session(
    session: StoredSession,
) -> Result<(AuthenticatedSession, Client), CliError> {
    let observer_api_url = url::Url::parse(&session.observer_api_url)
        .map_err(|e| CliError::Transport(format!("invalid stored observer_api_url: {e}")))?;
    crate::http::assert_url_allowed(&observer_api_url)?;
    let backend = ResolvedBackend {
        observer_api_url: observer_api_url.clone(),
        audience: AUDIENCE.to_string(),
        resolution_path: "device_session".to_string(),
        proxy_mode: true,
    };
    let client = Client::new(
        observer_api_url,
        TokenSource::static_token(session.cli_session_token),
    );
    Ok((
        AuthenticatedSession {
            platform: CiPlatform::Local,
            backend,
        },
        client,
    ))
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

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use crate::cli::LogFormat;
    use url::Url;

    #[tokio::test]
    async fn authenticate_uses_static_observer_token() {
        let _guard = crate::observer_token::ENV_LOCK.lock().expect("env lock");
        std::env::set_var("DESLICER_API_TOKEN", "dap_tools_ci_key");
        let ctx = Ctx {
            deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
            observer_api_url: Some(Url::parse("http://127.0.0.1:9").expect("observer")),
            ci_override: Some(CiPlatform::Github),
            log_format: LogFormat::Human,
        };
        let (session, _client) = authenticate(&ctx, Some("production"), None)
            .await
            .expect("authenticate");
        assert!(session.is_observer_api_token());
        assert!(!session.backend.proxy_mode);
        std::env::remove_var("DESLICER_API_TOKEN");
    }
}
