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
        let output = json!({
            "logged_in": session.is_active(),
            "identity": "device",
            "tenant_id": session.tenant_id,
            "display_name": session.display_name,
            "expires_at": session.expires_at,
        });
        print_output(
            ctx.log_format,
            &output,
            &whoami_device_human(
                session.is_active(),
                &session.display_name,
                &session.tenant_id,
                &session.expires_at,
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
