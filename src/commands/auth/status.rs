use base64::Engine;
use clap::Args as ClapArgs;
use serde_json::{json, Value};

use crate::auth_resolution::{AuthCredential, AuthCredentialResolver};
use crate::ci::{self, CiPlatform};
use crate::commands::auth::format::{
    print_output, status_ci_human, status_device_human, status_none_human, status_token_human,
};
use crate::commands::pipeline::map_cli_error;
use crate::reporting::{emit_oidc_error, oidc_exit_code, redact_secrets};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match AuthCredentialResolver::new(&ctx).resolve() {
        Ok(AuthCredential::DirectObserver { platform }) => print_direct_status(&ctx, platform),
        Ok(AuthCredential::CiOidc { platform }) => print_ci_status(ctx, args, platform).await,
        Ok(AuthCredential::Device(session) | AuthCredential::ExpiredDevice(session)) => {
            print_device_status(&ctx, &session)
        }
        Ok(AuthCredential::None) => print_no_identity(&ctx),
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn print_direct_status(ctx: &Ctx, platform: CiPlatform) -> i32 {
    let audience = ci::AUDIENCE;
    let url = ctx.observer_api_url.as_ref().map(|u| u.as_str());
    print_output(
        ctx.log_format,
        &json!({
            "ok": true,
            "platform": platform.header_value(),
            "identity": "observer_api_token",
            "observer_api_url": url,
            "resolution_path": crate::observer_token::RESOLUTION_PATH,
            "audience": audience,
        }),
        &status_token_human(url),
    );
    0
}

async fn print_ci_status(ctx: Ctx, args: Args, platform: CiPlatform) -> i32 {
    let audience = ci::AUDIENCE;
    let token_result = ci::provider_for(platform).fetch_token(audience).await;

    let (jwt_header, jwt_claims) = match &token_result {
        Ok(jwt) => decode_jwt_parts(jwt),
        Err(err) => {
            emit_oidc_error(ctx.log_format, err);
            (Value::Null, Value::Null)
        }
    };

    let resolved_backend = match &token_result {
        Ok(jwt) => {
            match crate::resolver::resolve(&ctx, jwt, platform, args.environment.as_deref(), None)
                .await
            {
                Ok(backend) => json!({
                    "observer_api_url": backend.observer_api_url.as_str(),
                    "resolution_path": backend.resolution_path,
                    "audience": backend.audience,
                }),
                Err(err) => json!(redact_secrets(&err.to_string())),
            }
        }
        Err(_) => Value::Null,
    };

    let ok = token_result.is_ok()
        && resolved_backend
            .get("observer_api_url")
            .and_then(Value::as_str)
            .is_some();

    let output = json!({
        "ok": ok,
        "platform": platform.header_value(),
        "identity": token_result.as_ref().ok().map(|_| "ci"),
        "audience": audience,
        "jwt_header": jwt_header,
        "jwt_claims": jwt_claims,
        "resolved_backend": resolved_backend,
    });

    let backend_url = resolved_backend
        .get("observer_api_url")
        .and_then(Value::as_str);
    let resolution_path = resolved_backend
        .get("resolution_path")
        .and_then(Value::as_str);
    print_output(
        ctx.log_format,
        &output,
        &status_ci_human(platform.header_value(), backend_url, resolution_path, ok),
    );

    if ok {
        0
    } else {
        token_result.as_ref().err().map(oidc_exit_code).unwrap_or(1)
    }
}

fn print_no_identity(ctx: &Ctx) -> i32 {
    print_output(
        ctx.log_format,
        &json!({
            "ok": false,
            "platform": "local",
            "identity": "none",
            "logged_in": false,
        }),
        &status_none_human(),
    );
    1
}

fn print_device_status(ctx: &Ctx, session: &crate::token_store::StoredSession) -> i32 {
    let logged_in = session.is_active();
    let output = json!({
        "ok": logged_in,
        "platform": "device",
        "identity": "device",
        "logged_in": logged_in,
        "tenant_id": session.tenant_id,
        "tenant_slug": session.tenant_slug,
        "display_name": session.display_name,
        "expires_at": session.expires_at,
        "observer_api_url": session.observer_api_url,
        "resolution_path": "device_session",
    });
    print_output(
        ctx.log_format,
        &output,
        &status_device_human(
            logged_in,
            &session.tenant_id,
            &session.display_name,
            &session.expires_at,
            &session.observer_api_url,
            session.tenant_slug.as_deref(),
        ),
    );
    if logged_in {
        0
    } else {
        1
    }
}

fn decode_jwt_parts(jwt: &str) -> (Value, Value) {
    let mut parts = jwt.split('.');
    let header = parts
        .next()
        .and_then(decode_jwt_segment)
        .unwrap_or(Value::Null);
    let mut claims = parts
        .next()
        .and_then(decode_jwt_segment)
        .unwrap_or(Value::Null);
    if !claims.is_null() {
        redact_sensitive_claims(&mut claims);
    }
    (header, claims)
}

fn decode_jwt_segment(segment: &str) -> Option<Value> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(segment)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn redact_sensitive_claims(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                let key_lower = key.to_ascii_lowercase();
                if key_lower.contains("token")
                    || key_lower.contains("secret")
                    || key_lower.contains("key")
                {
                    *val = Value::String("REDACTED".to_string());
                } else {
                    redact_sensitive_claims(val);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_sensitive_claims(item);
            }
        }
        _ => {}
    }
}
