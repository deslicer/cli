use clap::Args as ClapArgs;
use serde_json::json;

use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok((session, _client)) => {
            let output = json!({
                "platform": session.platform.header_value(),
                "observer_api_url": session.backend.observer_api_url.as_str(),
                "resolution_path": session.backend.resolution_path,
                "audience": session.backend.audience,
            });
            let text = match serde_json::to_string_pretty(&output) {
                Ok(s) => s,
                Err(_) => output.to_string(),
            };
            println!("{text}");
            0
        }
        Err(err) => map_cli_error(err),
    }
}
