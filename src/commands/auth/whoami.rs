use clap::Args as ClapArgs;
use serde_json::json;

use crate::ci::{self, CiPlatform};
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
        println!("{}", pretty(&output));
        return if session.is_active() { 0 } else { 1 };
    }

    let platform = ci::detect_platform(ctx.ci_override);
    if platform == CiPlatform::Local {
        println!(
            "{}",
            pretty(&json!({
                "logged_in": false,
                "identity": "none",
                "hint": "run `deslicer auth login` and approve the code in the portal",
            }))
        );
        return 1;
    }

    println!(
        "{}",
        pretty(&json!({
            "logged_in": true,
            "identity": "ci",
            "platform": platform.header_value(),
        }))
    );
    0
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}
