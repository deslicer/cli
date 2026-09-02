use std::io::IsTerminal;
use std::path::PathBuf;

use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::token_store::load_active_session;
use crate::Ctx;

use super::snippet::{fetch_worker_install_snippet, WorkerSnippetRequest};

#[derive(ClapArgs)]
pub struct Args {
    /// shell, ansible, or manual
    #[arg(long, default_value = "shell", value_parser = ["shell", "ansible", "manual"])]
    pub format: String,

    /// splunk-enterprise, splunkforwarder, or otel
    #[arg(long, default_value = "splunk-enterprise", value_parser = ["splunk-enterprise", "splunkforwarder", "otel"])]
    pub product: String,

    /// prod, staging, or development
    #[arg(long, default_value = "prod", value_parser = ["prod", "staging", "development"])]
    pub channel: String,

    /// Read a previously written enrollment token. Used only with --embed-token.
    #[arg(long)]
    pub token_file: Option<PathBuf>,

    /// Embed the token from --token-file in the snippet (TTY required).
    #[arg(long)]
    pub embed_token: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let log_format = ctx.log_format;
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(log_format, err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    let enrollment_token = resolve_embed_token(&args)?;
    let (session, _client) = authenticate(&ctx, None, None).await?;
    if !session.is_device_session() {
        return Err(CliError::Other(
            "`worker instructions` requires `deslicer auth login` (device session)".into(),
        ));
    }
    let stored = load_active_session()?.ok_or_else(|| {
        CliError::Other(
            "not logged in. Run `deslicer auth login` and approve the code in the portal".into(),
        )
    })?;

    let snippet = fetch_worker_install_snippet(
        &ctx,
        &stored,
        &WorkerSnippetRequest {
            format: args.format.clone(),
            product: args.product.clone(),
            channel: args.channel.clone(),
            enrollment_token,
        },
    )
    .await?;

    match ctx.log_format {
        LogFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "format": args.format,
                    "product": args.product,
                    "channel": args.channel,
                    "snippet": snippet,
                })
            );
        }
        LogFormat::Human => {
            print!("{snippet}");
            if !snippet.ends_with('\n') {
                println!();
            }
        }
    }
    Ok(0)
}

fn resolve_embed_token(args: &Args) -> Result<Option<String>, CliError> {
    if !args.embed_token {
        return Ok(None);
    }
    if !std::io::stdout().is_terminal() {
        return Err(CliError::Other(
            "--embed-token requires an interactive terminal".into(),
        ));
    }
    let path = args
        .token_file
        .as_deref()
        .ok_or_else(|| CliError::Other("--embed-token requires --token-file".into()))?;
    let raw = std::fs::read_to_string(path)
        .map_err(|err| CliError::Other(format!("read --token-file: {err}")))?;
    let token = raw.trim().to_string();
    if token.is_empty() {
        return Err(CliError::Other("--token-file is empty".into()));
    }
    Ok(Some(token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn embed_token_requires_token_file() {
        let err = resolve_embed_token(&Args {
            format: "shell".into(),
            product: "splunk-enterprise".into(),
            channel: "prod".into(),
            token_file: None,
            embed_token: true,
        });
        match err {
            Ok(_) if !std::io::stdout().is_terminal() => {}
            Err(e) => {
                let text = e.to_string();
                assert!(
                    text.contains("--token-file") || text.contains("interactive terminal"),
                    "{text}"
                );
            }
            Ok(_) => panic!("embed without token-file should fail on a TTY"),
        }
    }

    #[test]
    fn default_omits_token() {
        let token = resolve_embed_token(&Args {
            format: "shell".into(),
            product: "splunk-enterprise".into(),
            channel: "prod".into(),
            token_file: Some(PathBuf::from("/tmp/unused")),
            embed_token: false,
        })
        .expect("omit");
        assert!(token.is_none());
    }

    #[test]
    fn help_has_no_realistic_token() {
        let mut cmd = crate::cli::Cli::command();
        let worker = cmd.find_subcommand_mut("worker").expect("worker");
        let instructions = worker
            .find_subcommand_mut("instructions")
            .expect("instructions");
        let mut buf = Vec::new();
        instructions.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(!help.contains("dsle_enroll_"));
        assert!(help.contains("--token-file"));
        assert!(help.contains("--embed-token"));
    }
}
