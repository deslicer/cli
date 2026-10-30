use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Catalog blueprint id (from `catalog list`)
    pub id: String,

    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let entry = match client.show_catalog_entry(&args.id).await {
        Ok(entry) => entry,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(&entry) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize catalog entry: {err}");
                1
            }
        },
        LogFormat::Human => {
            println!("id: {}", entry.id);
            if let Some(name) = entry.name.as_deref() {
                println!("name: {name}");
            }
            if let Some(version) = entry.version.as_deref() {
                println!("version: {version}");
            }
            if let Some(kind) = entry.kind.as_deref() {
                println!("kind: {kind}");
            }
            if let Some(description) = entry.description.as_deref() {
                println!("description: {description}");
            }
            0
        }
    }
}
