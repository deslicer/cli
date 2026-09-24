use clap::Args as ClapArgs;
use serde_json::json;

use crate::auth_resolution::{AuthCredential, AuthCredentialResolver};
use crate::commands::auth::format::{login_human, print_output};
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::interactive;
use crate::token_store::{CompositeTokenStore, TokenStore};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let credential = match AuthCredentialResolver::new(&ctx).resolve() {
        Ok(credential) => credential,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    if matches!(
        credential,
        AuthCredential::ExpiredDevice(_) | AuthCredential::None
    ) {
        return if interactive::is_non_interactive() {
            map_cli_error(ctx.log_format, local_login_error())
        } else {
            device_login(ctx).await
        };
    }

    match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok((session, _client)) => {
            print_login(
                ctx,
                session.platform.header_value(),
                session.backend.observer_api_url.as_str(),
                &session.backend.resolution_path,
                &session.backend.audience,
            );
            0
        }
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

async fn device_login(ctx: Ctx) -> i32 {
    match crate::device_flow::login_device_session(&ctx).await {
        Ok(session) => {
            if let Err(err) =
                CompositeTokenStore::default_store().and_then(|store| store.save(&session))
            {
                return map_cli_error(ctx.log_format, err);
            }
            print_login(
                ctx,
                "device",
                &session.observer_api_url,
                "device_session",
                crate::ci::AUDIENCE,
            );
            0
        }
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn local_login_error() -> CliError {
    CliError::Other(
        "cannot run interactive device login without a TTY (CI=1 or non-interactive stdout/stdin). \
         For automation, set OBSERVER_API_URL and DESLICER_API_TOKEN, or use real CI OIDC \
         with --ci-platform github|gitlab|azure|bitbucket."
            .into(),
    )
}

fn print_login(
    ctx: Ctx,
    identity: &str,
    observer_api_url: &str,
    resolution_path: &str,
    audience: &str,
) {
    let output = json!({
        "platform": identity,
        "observer_api_url": observer_api_url,
        "resolution_path": resolution_path,
        "audience": audience,
    });
    print_output(
        ctx.log_format,
        &output,
        &login_human(identity, observer_api_url, resolution_path),
    );
}
