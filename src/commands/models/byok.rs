use clap::{Args as ClapArgs, Subcommand};
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::models::client::ModelsClient;
use crate::commands::pipeline::map_cli_error;
use crate::Ctx;

#[derive(Subcommand)]
pub enum ByokCmd {
    /// Show BYOK configuration status
    Status(StatusArgs),
    /// Set a BYOK provider key (dry-run unless --yes)
    Set(SetArgs),
}

#[derive(ClapArgs)]
pub struct StatusArgs {}

#[derive(ClapArgs)]
pub struct SetArgs {
    /// Provider id (e.g. openai, anthropic)
    #[arg(long)]
    pub provider: String,

    /// API key for the provider
    #[arg(long)]
    pub key: String,

    /// Apply the key. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn dispatch(ctx: Ctx, cmd: ByokCmd) -> i32 {
    match cmd {
        ByokCmd::Status(args) => status(ctx, args).await,
        ByokCmd::Set(args) => set(ctx, args).await,
    }
}

async fn status(ctx: Ctx, _args: StatusArgs) -> i32 {
    let client = match ModelsClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let status = match client.byok_status().await {
        Ok(status) => status,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&status) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize byok status: {err}");
                1
            }
        },
        LogFormat::Human => {
            if let Some(configured) = status.configured {
                println!("configured: {}", if configured { "yes" } else { "no" });
            }
            if let Some(provider) = status.provider.as_deref() {
                println!("provider: {provider}");
            }
            if let Some(hint) = status.key_hint.as_deref() {
                println!("key: {hint}");
            }
            if let Some(s) = status.status.as_deref() {
                println!("status: {s}");
            }
            if let Some(msg) = status.message.as_deref() {
                println!("{msg}");
            }
            0
        }
    }
}

async fn set(ctx: Ctx, args: SetArgs) -> i32 {
    if !args.yes {
        let hint = redact_key(&args.key);
        match ctx.log_format {
            LogFormat::Json => println!(
                "{}",
                json!({
                    "dry_run": true,
                    "action": "byok_set",
                    "provider": args.provider,
                    "key_hint": hint,
                })
            ),
            LogFormat::Human => println!(
                "dry-run: would set BYOK for provider {} (key {hint})\npass --yes to apply",
                args.provider
            ),
        }
        return 0;
    }

    let client = match ModelsClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match client.byok_set(&args.provider, &args.key).await {
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
                    eprintln!("failed to serialize byok set: {err}");
                    1
                }
            },
            LogFormat::Human => {
                if result.ok {
                    println!("BYOK set for provider {}", args.provider);
                } else {
                    println!("BYOK set failed for provider {}", args.provider);
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
    fn help_requires_yes_for_set() {
        let mut cmd = crate::cli::Cli::command();
        let models = cmd.find_subcommand_mut("models").expect("models");
        let byok = models.find_subcommand_mut("byok").expect("byok");
        let set = byok.find_subcommand_mut("set").expect("set");
        let mut buf = Vec::new();
        set.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(help.contains("--yes"));
        assert!(help.contains("--provider"));
        assert!(help.contains("--key"));
    }

    #[test]
    fn redacts_key() {
        assert_eq!(redact_key("sk-abcdefgh"), "****efgh");
    }
}
