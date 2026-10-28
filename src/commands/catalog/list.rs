use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::CatalogEntry;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let entries = match client.list_catalog().await {
        Ok(entries) => entries,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&entries) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize catalog: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_catalog_human(&entries));
            0
        }
    }
}

fn format_catalog_human(entries: &[CatalogEntry]) -> String {
    if entries.is_empty() {
        return "No catalog blueprints.\n".to_string();
    }
    let mut lines = vec!["ID  NAME  VERSION  KIND".to_string()];
    for e in entries {
        lines.push(format!(
            "{}  {}  {}  {}",
            e.id,
            e.name.as_deref().unwrap_or("-"),
            e.version.as_deref().unwrap_or("-"),
            e.kind.as_deref().unwrap_or("-")
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_list_rows() {
        let text = format_catalog_human(&[CatalogEntry {
            id: "bp1".into(),
            name: Some("Indexer baseline".into()),
            description: None,
            version: Some("2".into()),
            kind: Some("blueprint".into()),
        }]);
        assert!(text.contains("bp1"));
        assert!(text.contains("Indexer baseline"));
    }
}
