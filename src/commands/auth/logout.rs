use clap::Args as ClapArgs;
use serde_json::json;

use crate::commands::auth::format::print_output;
use crate::commands::pipeline::map_cli_error;
use crate::token_store::{CompositeTokenStore, TokenStore};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    match CompositeTokenStore::default_store().and_then(|store| store.clear()) {
        Ok(()) => {
            print_output(
                ctx.log_format,
                &json!({ "ok": true, "logged_out": true }),
                "Logged out\n",
            );
            0
        }
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}
