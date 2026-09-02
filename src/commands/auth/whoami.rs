use clap::Args as ClapArgs;
use serde_json::json;

use crate::ci::{self, CiPlatform};
use crate::commands::auth::format::{
    print_output, whoami_ci_human, whoami_device_human, whoami_none_human, whoami_token_human,
};
use crate::token_store::load_stored_session;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    if let Ok(Some(session)) = load_stored_session() {
        let output = device_whoami_json(&session);
        print_output(
            ctx.log_format,
            &output,
            &whoami_device_human(
                session.is_active(),
                &session.display_name,
                &session.tenant_id,
                &session.expires_at,
                session.tenant_slug.as_deref(),
            ),
        );
        return if session.is_active() { 0 } else { 1 };
    }

    if crate::observer_token::direct_auth_ready(&ctx) {
        let url = ctx.observer_api_url.as_ref().map(|u| u.as_str());
        print_output(
            ctx.log_format,
            &json!({
                "logged_in": true,
                "identity": "observer_api_token",
                "observer_api_url": url,
            }),
            &whoami_token_human(url),
        );
        return 0;
    }

    let platform = ci::detect_platform(ctx.ci_override);
    if platform == CiPlatform::Local {
        print_output(
            ctx.log_format,
            &json!({
                "logged_in": false,
                "identity": "none",
                "hint": "run `deslicer auth login` and approve the code in the portal",
            }),
            &whoami_none_human(),
        );
        return 1;
    }

    print_output(
        ctx.log_format,
        &json!({
            "logged_in": true,
            "identity": "ci",
            "platform": platform.header_value(),
        }),
        &whoami_ci_human(platform.header_value()),
    );
    0
}

fn device_whoami_json(session: &crate::token_store::StoredSession) -> serde_json::Value {
    json!({
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
