use clap::Args as ClapArgs;
use serde_json::json;

use crate::token_store::{CompositeTokenStore, TokenStore};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(_ctx: Ctx, _args: Args) -> i32 {
    match CompositeTokenStore::default_store().and_then(|store| store.clear()) {
        Ok(()) => {
            println!("{}", json!({ "logged_out": true }));
            0
        }
        Err(err) => {
            eprintln!("{err}");
            err.exit_code()
        }
    }
}
