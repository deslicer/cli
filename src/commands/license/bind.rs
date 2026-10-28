use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// License key to bind to this tenant
    #[arg(long)]
    pub key: String,

    #[arg(long)]
    pub environment: Option<String>,

    /// Perform the bind. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    if !args.yes {
        let hint = redact_key(&args.key);
        match ctx.log_format {
            LogFormat::Json => println!(
                "{}",
                json!({
                    "dry_run": true,
                    "action": "license_bind",
                    "key_hint": hint,
                })
            ),
            LogFormat::Human => {
                println!("dry-run: would bind license key {hint}\npass --yes to bind")
            }
        }
        return 0;
    }

    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client.license_bind(&args.key).await {
        Ok(result) => match ctx.log_format {
            LogFormat::Json => match serde_json::to_string_pretty(&result) {
                Ok(json) => {
                    println!("{json}");
                    if result.ok {
                        0
                    } else {
                        1
                    }
                }
                Err(err) => {
                    eprintln!("failed to serialize bind result: {err}");
                    1
                }
            },
            LogFormat::Human => {
                if result.ok {
                    println!("license bound");
                } else {
                    println!("license bind failed");
                }
                if let Some(msg) = result.message.as_deref() {
                    println!("{msg}");
                }
                if result.ok {
                    0
                } else {
                    1
                }
            }
        },
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

fn redact_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.len() <= 4 {
        return "****".to_string();
    }
    format!("****{}", &trimmed[trimmed.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn redacts_key() {
        assert_eq!(redact_key("ABCDEFGH"), "****EFGH");
        assert_eq!(redact_key("ab"), "****");
    }

    #[test]
    fn help_requires_yes() {
        let mut cmd = crate::cli::Cli::command();
        let license = cmd.find_subcommand_mut("license").expect("license");
        let bind = license.find_subcommand_mut("bind").expect("bind");
        let mut buf = Vec::new();
        bind.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--yes"));
        assert!(help.contains("--key"));
    }
}
