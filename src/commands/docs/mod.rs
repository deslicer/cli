//! Print (or open) CLI documentation topics.
//!
//! Deep guides stay in `docs/*.md`. This command is a thin index so operators
//! can run `deslicer docs path-a2` after `init` without hunting GitHub.

use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::Ctx;

mod catalog;
mod open;
mod urls;

pub use catalog::{lookup, Topic, TOPICS};

use catalog::known_topic_ids;
use urls::{docs_base_for_list, topic_url};

#[derive(ClapArgs)]
#[command(after_long_help = DOCS_EXAMPLES)]
pub struct Args {
    /// Topic id (`quickstart`, `path-a2`, `init`, …). Omit to list topics.
    pub topic: Option<String>,

    /// Print only the URL (no title or summary).
    #[arg(long)]
    pub print: bool,

    /// Open the topic in a browser (skipped in CI unless you pass this).
    #[arg(long)]
    pub open: bool,
}

const DOCS_EXAMPLES: &str = "\
Examples:
  deslicer docs
  deslicer docs path-a2
  deslicer docs init --print
  deslicer docs quickstart --open
  deslicer docs api-keys --open

Hosted site (optional):
  DESLICER_DOCS_BASE_URL=https://docs.deslicer.io/cli deslicer docs path-a2
";

pub fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(&ctx, args) {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

fn run_inner(ctx: &Ctx, args: Args) -> Result<(), String> {
    match args.topic.as_deref() {
        None => print_list(ctx, args.print),
        Some(name) => print_topic(ctx, name, args.print, args.open),
    }
}

fn print_list(ctx: &Ctx, url_only: bool) -> Result<(), String> {
    let base = docs_base_for_list();
    match ctx.log_format {
        LogFormat::Json => {
            let topics: Vec<serde_json::Value> = TOPICS
                .iter()
                .map(|topic| {
                    serde_json::json!({
                        "id": topic.id,
                        "aliases": topic.aliases,
                        "title": topic.title,
                        "summary": topic.summary,
                        "url": topic_url(topic, &ctx.deslicer_api_url),
                        "sync": topic.sync,
                    })
                })
                .collect();
            let payload = serde_json::json!({
                "base_url": base,
                "topics": topics,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
            );
        }
        LogFormat::Human if url_only => {
            for topic in TOPICS {
                println!("{}", topic_url(topic, &ctx.deslicer_api_url));
            }
        }
        LogFormat::Human => {
            println!("Deslicer CLI docs  ({base})");
            println!();
            for topic in TOPICS {
                let extra = if topic.sync { "" } else { "  [internal]" };
                println!("  {:<16} {}{extra}", topic.id, topic.title);
                println!("                   {}", topic.summary);
            }
            println!();
            println!("Usage: deslicer docs <topic>   (--print for URL only, --open for browser)");
        }
    }
    Ok(())
}

fn print_topic(ctx: &Ctx, name: &str, url_only: bool, open_browser: bool) -> Result<(), String> {
    let Some(topic) = lookup(name) else {
        return Err(format!(
            "unknown docs topic {name:?}. Try one of: {}",
            known_topic_ids().join(", ")
        ));
    };
    let url = topic_url(topic, &ctx.deslicer_api_url);
    match ctx.log_format {
        LogFormat::Json => {
            let payload = serde_json::json!({
                "id": topic.id,
                "title": topic.title,
                "summary": topic.summary,
                "url": url,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
            );
        }
        LogFormat::Human if url_only => println!("{url}"),
        LogFormat::Human => {
            println!("{} — {}", topic.title, topic.summary);
            println!("{url}");
        }
    }
    if open_browser {
        open::open_url(&url, Some(&ctx.deslicer_api_url))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ci::CiPlatform;
    use url::Url;

    fn ctx(format: LogFormat) -> Ctx {
        Ctx {
            deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
            observer_api_url: None,
            ci_override: Some(CiPlatform::Local),
            log_format: format,
        }
    }

    #[test]
    fn unknown_topic_errors() {
        let err = print_topic(&ctx(LogFormat::Human), "nope", true, false).unwrap_err();
        assert!(err.contains("unknown docs topic"));
        assert!(err.contains("path-a2"));
    }

    #[test]
    fn known_topic_ok() {
        print_topic(&ctx(LogFormat::Human), "path-a2", true, false).expect("print");
    }

    #[test]
    fn api_keys_topic_ok() {
        print_topic(&ctx(LogFormat::Human), "api-keys", true, false).expect("print");
    }
}
