use clap::Args as ClapArgs;
use serde_json::json;

use crate::auth_resolution::{AuthCredential, AuthCredentialResolver};
use crate::ci::{self, CiPlatform, AUDIENCE};
use crate::commands::auth::format::{
    print_output, whoami_ci_human, whoami_device_human, whoami_none_human, whoami_token_human,
};
use crate::commands::auth::probe::identity_probe_forbidden;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::reporting::{emit_cli_error, emit_oidc_error, oidc_exit_code};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    match AuthCredentialResolver::new(&ctx).resolve() {
        Ok(AuthCredential::DirectObserver { .. }) => print_direct_whoami(ctx).await,
        Ok(AuthCredential::CiOidc { platform }) => print_ci_whoami(&ctx, platform).await,
        Ok(AuthCredential::Device(session) | AuthCredential::ExpiredDevice(session)) => {
            print_device_whoami(&ctx, &session)
        }
        Ok(AuthCredential::None) => print_no_identity(&ctx),
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

/// Probe Observer with the static API token so a rejected key is not reported
/// as logged in (issue #209). Uses `GET api/v1/me` (identity), not a resource
/// read like groups — a valid narrowly scoped key must not look logged-out.
async fn print_direct_whoami(ctx: Ctx) -> i32 {
    let url = ctx
        .observer_api_url
        .as_ref()
        .map(|u| u.as_str().to_string());
    let (_session, client) = match authenticate(&ctx, None, None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client.get_me().await {
        Ok(_) => print_token_logged_in(&ctx, url.as_deref()),
        // 403: auth accepted the bearer; insufficient scope / forbid is not
        // "not logged in" (that is 401 → ApiTokenRejected / OidcRejected only).
        Err(err) if identity_probe_forbidden(&err) => print_token_logged_in(&ctx, url.as_deref()),
        Err(err @ (CliError::ApiTokenRejected(_) | CliError::OidcRejected(_))) => {
            // Single document only: do not also emit_cli_error (that printed a
            // second JSON object on stderr and broke `--log-format json` parsers).
            print_output(
                ctx.log_format,
                &json!({
                    "ok": false,
                    "logged_in": false,
                    "identity": "none",
                    "observer_api_url": url,
                    "hint": "DESLICER_API_TOKEN was rejected by Observer",
                }),
                &whoami_none_human(),
            );
            err.exit_code()
        }
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn print_token_logged_in(ctx: &Ctx, url: Option<&str>) -> i32 {
    print_output(
        ctx.log_format,
        &json!({
            "ok": true,
            "logged_in": true,
            "identity": "observer_api_token",
            "observer_api_url": url,
        }),
        &whoami_token_human(url),
    );
    0
}

async fn print_ci_whoami(ctx: &Ctx, platform: CiPlatform) -> i32 {
    match ci::provider_for(platform).fetch_token(AUDIENCE).await {
        Ok(_) => {
            print_output(
                ctx.log_format,
                &json!({
                    "ok": true,
                    "logged_in": true,
                    "identity": "ci",
                    "platform": platform.header_value(),
                }),
                &whoami_ci_human(platform.header_value()),
            );
            0
        }
        Err(err) => {
            emit_oidc_error(ctx.log_format, &err);
            print_output(
                ctx.log_format,
                &json!({
                    "ok": false,
                    "logged_in": false,
                    "identity": "none",
                    "platform": platform.header_value(),
                }),
                &whoami_none_human(),
            );
            oidc_exit_code(&err)
        }
    }
}

fn print_device_whoami(ctx: &Ctx, session: &crate::token_store::StoredSession) -> i32 {
    if !session.is_active() {
        // Expired session → auth exit 4 + envelope (issue #236), not exit 1
        // with a non-envelope document on stdout.
        return emit_cli_error(ctx.log_format, &not_logged_in_error());
    }
    let output = device_whoami_json(session);
    print_output(
        ctx.log_format,
        &output,
        &whoami_device_human(
            true,
            &session.display_name,
            &session.tenant_id,
            &session.expires_at,
            session.tenant_slug.as_deref(),
        ),
    );
    0
}

fn print_no_identity(ctx: &Ctx) -> i32 {
    // No credential → auth exit 4 with the error envelope on stderr.
    // Do not print a non-envelope JSON status document on stdout (issue #236).
    emit_cli_error(ctx.log_format, &not_logged_in_error())
}

fn not_logged_in_error() -> CliError {
    CliError::Other(
        "not logged in. Run `deslicer auth login` and approve the code in the portal".into(),
    )
}

fn device_whoami_json(session: &crate::token_store::StoredSession) -> serde_json::Value {
    json!({
        "ok": session.is_active(),
        "logged_in": session.is_active(),
        "identity": "device",
        "tenant_id": session.tenant_id,
        "tenant_slug": session.tenant_slug,
        "display_name": session.display_name,
        "expires_at": session.expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_store::StoredSession;

    fn session(slug: Option<&str>) -> StoredSession {
        StoredSession {
            cli_session_token: "dslcli_x".into(),
            expires_at: "2099-01-01T00:00:00.000Z".into(),
            tenant_id: "2fb5ef22-12ad-4d20-9e0f-4736f47953bb".into(),
            display_name: "Ada".into(),
            observer_api_url: "https://ops.deslicer.show/api/cli/observer/".into(),
            tenant_slug: slug.map(str::to_string),
            deslicer_api_url: None,
        }
    }

    #[test]
    fn whoami_json_includes_tenant_slug() {
        let with_slug = device_whoami_json(&session(Some("dap-102")));
        assert_eq!(with_slug["tenant_slug"], "dap-102");
        assert_eq!(
            with_slug["tenant_id"],
            "2fb5ef22-12ad-4d20-9e0f-4736f47953bb"
        );

        let without = device_whoami_json(&session(None));
        assert!(without["tenant_slug"].is_null());
    }
}
