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
    /// Environment name used to resolve CI bindings in status output.
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
    let drift = api_url_drift(ctx, session);
    let mut output = json!({
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
        "deslicer_api_url": crate::session_portal::resolve_deslicer_api_url(ctx, session).as_str(),
        "session_deslicer_api_url": session.deslicer_api_url,
        "cli_deslicer_api_url": ctx.deslicer_api_url.as_str(),
    });
    if let Some(warning) = drift.as_ref() {
        output["api_url_drift"] = json!(true);
        output["warning"] = json!(warning);
        // Drift is a warning, not a hard failure, while the session is active.
        if logged_in {
            output["ok"] = json!(true);
        }
    } else {
        output["api_url_drift"] = json!(false);
    }
    let mut human = status_device_human(
        logged_in,
        &session.tenant_id,
        &session.display_name,
        &session.expires_at,
        &session.observer_api_url,
        session.tenant_slug.as_deref(),
    );
    if let Some(warning) = drift.as_ref() {
        human.push_str(&format!("Warning: {warning}\n"));
    }
    print_output(ctx.log_format, &output, &human);
    if logged_in {
        0
    } else {
        1
    }
}

/// Warn when the CLI flag/env API URL drifts from the portal that issued the session.
///
/// Related: DAI #2846 / parity #155. Operators who log into show/enterprise then
/// run with the default SaaS `--deslicer-api-url` get confusing 404s from agent.
pub(crate) fn api_url_drift(
    ctx: &Ctx,
    session: &crate::token_store::StoredSession,
) -> Option<String> {
    let session_portal = crate::session_portal::portal_url(session)?;
    let effective = crate::session_portal::resolve_deslicer_api_url(ctx, session);
    let explicit = crate::session_portal::deslicer_api_url_is_explicit();

    // When the flag/env is not explicit, resolve_deslicer_api_url already prefers
    // the session portal — no drift for the effective URL. Still warn if the
    // *default* CLI URL would have drifted (informational about the session).
    if explicit {
        let requested = normalize_origin(&ctx.deslicer_api_url);
        let portal = normalize_origin(&session_portal);
        if requested != portal {
            return Some(format!(
                "API URL drift: --deslicer-api-url / DESLICER_API_URL is {requested}, \
                 but this session was issued by {portal}. Agent and org commands use the \
                 explicit flag; re-login on the intended portal or drop the override."
            ));
        }
    } else {
        let default_portal = normalize_origin(&ctx.deslicer_api_url);
        let session_origin = normalize_origin(&session_portal);
        let _ = effective; // resolve prefers the session portal when the flag is unset
        if default_portal != session_origin {
            return Some(format!(
                "API URL drift: session portal is {session_origin}, while the CLI default \
                 is {default_portal}. Commands use the session portal automatically; pass \
                 --deslicer-api-url {session_origin} to make that explicit, or re-login."
            ));
        }
    }
    None
}

fn normalize_origin(url: &url::Url) -> String {
    let origin = url.origin().ascii_serialization();
    // Strip trailing slash variance for comparison display.
    origin.trim_end_matches('/').to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::LogFormat;
    use crate::token_store::StoredSession;

    fn session(observer: &str, portal: Option<&str>) -> StoredSession {
        StoredSession {
            cli_session_token: "dslcli_abc".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            tenant_id: "tenant".into(),
            display_name: "Ada".into(),
            observer_api_url: observer.into(),
            tenant_slug: None,
            deslicer_api_url: portal.map(str::to_string),
        }
    }

    fn ctx(url: &str) -> Ctx {
        Ctx {
            deslicer_api_url: url::Url::parse(url).unwrap(),
            observer_api_url: None,
            ci_override: None,
            log_format: LogFormat::Human,
        }
    }

    #[test]
    fn warns_when_session_portal_differs_from_default() {
        // Ensure we exercise the non-explicit path.
        std::env::remove_var("DESLICER_API_URL");
        let session = session(
            "https://ops.deslicer.show/api/cli/observer/",
            Some("https://ops.deslicer.show/"),
        );
        let ctx = ctx("https://api.deslicer.ai/");
        let warning = api_url_drift(&ctx, &session).expect("drift");
        assert!(warning.contains("API URL drift"));
        assert!(warning.contains("ops.deslicer.show"));
        assert!(warning.contains("api.deslicer.ai"));
    }

    #[test]
    fn no_warning_when_session_matches_default() {
        std::env::remove_var("DESLICER_API_URL");
        let session = session(
            "https://api.deslicer.ai/api/cli/observer/",
            Some("https://api.deslicer.ai/"),
        );
        let ctx = ctx("https://api.deslicer.ai/");
        assert!(api_url_drift(&ctx, &session).is_none());
    }
}
