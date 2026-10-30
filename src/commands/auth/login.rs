use clap::Args as ClapArgs;
use serde_json::json;

use crate::auth_resolution::{AuthCredential, AuthCredentialResolver};
use crate::commands::auth::format::{login_human, login_unverified_human, print_output};
use crate::commands::auth::probe::{identity_probe_forbidden, identity_probe_unavailable};
use crate::commands::pipeline::{authenticate, map_cli_error, AuthenticatedSession};
use crate::errors::CliError;
use crate::interactive;
use crate::observer_client::Client;
use crate::token_store::{CompositeTokenStore, TokenStore};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Environment name used to resolve CI bindings during login.
    #[arg(long)]
    pub environment: Option<String>,

    /// Print the device code as JSON and exit without polling (agents).
    #[arg(long, conflicts_with = "wait")]
    pub no_wait: bool,

    /// Poll until the device code is approved (default on a TTY).
    #[arg(long, conflicts_with = "no_wait")]
    pub wait: bool,
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
        return if interactive::is_non_interactive() && !args.wait && !args.no_wait {
            // Fail fast for automation: do not start a portal device flow against
            // a remote host without an explicit --wait / --no-wait (preserves #237 / D1).
            // Agents pass --no-wait to start the flow, print codes, and exit pending.
            map_cli_error(ctx.log_format, local_login_error())
        } else {
            device_login(ctx, should_poll(&args)).await
        };
    }

    match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok((session, client)) => confirm_login(ctx, session, client).await,
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn should_poll(args: &Args) -> bool {
    if args.no_wait {
        return false;
    }
    if args.wait {
        return true;
    }
    !interactive::is_non_interactive()
}

/// For static API tokens, probe `GET /me` before claiming "Logged in" (issue #237 / #217).
///
/// Missing `/me` (exit-15 class) is soft success: logged in, unverified (issue #246).
async fn confirm_login(ctx: Ctx, session: AuthenticatedSession, client: Client) -> i32 {
    let mut verified = true;
    if session.is_observer_api_token() {
        match client.get_me().await {
            Ok(_) => {}
            Err(err) if identity_probe_forbidden(&err) => {}
            Err(err) if identity_probe_unavailable(&err) => {
                verified = false;
            }
            Err(err @ (CliError::ApiTokenRejected(_) | CliError::OidcRejected(_))) => {
                return map_cli_error(ctx.log_format, err);
            }
            Err(err) => return map_cli_error(ctx.log_format, err),
        }
    }

    print_login(
        ctx,
        session.platform.header_value(),
        session.backend.observer_api_url.as_str(),
        &session.backend.resolution_path,
        &session.backend.audience,
        verified,
    );
    0
}

async fn device_login(ctx: Ctx, poll: bool) -> i32 {
    match crate::device_flow::login_device_session_with_poll(&ctx, poll).await {
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
                true,
            );
            0
        }
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn local_login_error() -> CliError {
    // Prefix with "not logged in" so exit_code → 4 / auth.not_logged_in.
    // Avoid the substring "OIDC" so reporting does not mis-label this as auth.oidc
    // (issue #237). Hint names DESLICER_API_TOKEN via cli_error_hint.
    CliError::Other(
        "not logged in: cannot run interactive device login without a TTY \
         (CI=1, --no-input, or non-interactive stdout/stdin). Set OBSERVER_API_URL and \
         DESLICER_API_TOKEN for automation, or run from a real terminal \
         (pass --wait to start a device login and poll)."
            .into(),
    )
}

fn print_login(
    ctx: Ctx,
    identity: &str,
    observer_api_url: &str,
    resolution_path: &str,
    audience: &str,
    verified: bool,
) {
    let output = json!({
        "platform": identity,
        "observer_api_url": observer_api_url,
        "resolution_path": resolution_path,
        "audience": audience,
        "details": { "verified": verified },
    });
    let human = if verified {
        login_human(identity, observer_api_url, resolution_path)
    } else {
        login_unverified_human(identity, observer_api_url, resolution_path)
    };
    print_output(ctx.log_format, &output, &human);
}
