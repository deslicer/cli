use std::io::{IsTerminal, Read};

use clap::{Args as ClapArgs, Subcommand};
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::{validate_acs_environment, AcsCredential};
use crate::Ctx;

/// Environment variable read by `acs credentials set` when `--token-stdin` is not given.
pub const ACS_TOKEN_ENV: &str = "DESLICER_ACS_TOKEN";

#[derive(Subcommand)]
pub enum CredentialsCmd {
    /// List ACS stack credentials (metadata only; tokens are never shown)
    ///
    /// Examples:
    ///   deslicer acs credentials list
    ///   deslicer --log-format json acs credentials list
    List(ListArgs),
    /// Test that the stored credential for a stack can reach ACS (read-only observe)
    ///
    /// Exit 0 when ACS answers, 1 when Observer reports the credential missing
    /// or rejected.
    ///
    /// Examples:
    ///   deslicer acs credentials test --name my-stack
    Test(TestArgs),
    /// Store or rotate the ACS JWT for a stack (dry-run unless --yes)
    ///
    /// The token is read from stdin (--token-stdin) or DESLICER_ACS_TOKEN and is
    /// never accepted on the command line, printed or logged.
    ///
    /// Examples:
    ///   printf '%s' "$ACS_JWT" | deslicer acs credentials set my-stack --token-stdin --yes
    ///   DESLICER_ACS_TOKEN=... deslicer acs credentials set my-stack --acs-environment staging --yes
    Set(SetArgs),
}

#[derive(ClapArgs)]
pub struct ListArgs {
    /// Deslicer environment profile used for auth
    #[arg(long)]
    pub environment: Option<String>,
}

#[derive(ClapArgs)]
pub struct TestArgs {
    /// ACS stack name as shown by `acs credentials list`
    #[arg(long)]
    pub name: String,

    /// Deslicer environment profile used for auth
    #[arg(long)]
    pub environment: Option<String>,
}

#[derive(ClapArgs)]
pub struct SetArgs {
    /// ACS stack name
    pub stack: String,

    /// Read the ACS JWT from stdin instead of DESLICER_ACS_TOKEN
    #[arg(long)]
    pub token_stdin: bool,

    /// ACS environment for the stack: production (default) or staging
    #[arg(long, value_parser = ["production", "staging"])]
    pub acs_environment: Option<String>,

    /// Store the credential. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,

    /// Deslicer environment profile used for auth
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn dispatch(ctx: Ctx, cmd: CredentialsCmd) -> i32 {
    match cmd {
        CredentialsCmd::List(args) => list(ctx, args).await,
        CredentialsCmd::Test(args) => test(ctx, args).await,
        CredentialsCmd::Set(args) => set(ctx, args).await,
    }
}

async fn list(ctx: Ctx, args: ListArgs) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    let credentials = match client.list_acs_credentials().await {
        Ok(credentials) => credentials,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&credentials) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize credentials: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_credentials_human(&credentials));
            0
        }
    }
}

async fn test(ctx: Ctx, args: TestArgs) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    let result = match client.test_splunk_credential(&args.name).await {
        Ok(result) => result,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let code = if result.ok { 0 } else { 1 };

    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&result) {
            Ok(json) => {
                println!("{json}");
                code
            }
            Err(err) => {
                eprintln!("failed to serialize test result: {err}");
                1
            }
        },
        LogFormat::Human => {
            if result.ok {
                println!(
                    "credential for stack {}: ok ({} object(s) visible)",
                    result.stack,
                    result.object_count.unwrap_or(0)
                );
            } else {
                println!(
                    "credential for stack {}: failed — {}",
                    result.stack,
                    result.message.as_deref().unwrap_or("unknown error")
                );
            }
            code
        }
    }
}

async fn set(ctx: Ctx, args: SetArgs) -> i32 {
    if let Some(env) = args.acs_environment.as_deref() {
        if let Err(err) = validate_acs_environment(env) {
            return map_cli_error(ctx.log_format, err);
        }
    }
    if !args.yes {
        print_set_dry_run(&ctx, &args);
        return 0;
    }
    let token = match read_token(args.token_stdin) {
        Ok(token) => token,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let cred = match client
        .set_acs_credential(&args.stack, &token, args.acs_environment.as_deref())
        .await
    {
        Ok(cred) => cred,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&cred) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize credential: {err}");
                1
            }
        },
        LogFormat::Human => {
            println!(
                "stored ACS credential for stack {} ({})",
                cred.stack_name,
                cred.environment.as_deref().unwrap_or("production")
            );
            0
        }
    }
}

fn read_token(from_stdin: bool) -> Result<String, CliError> {
    let raw = if from_stdin {
        if std::io::stdin().is_terminal() {
            return Err(CliError::InvalidInput(
                "--token-stdin expects the ACS token on a pipe, not a terminal".into(),
            ));
        }
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| CliError::InvalidInput(format!("failed to read token from stdin: {e}")))?;
        buf
    } else {
        std::env::var(ACS_TOKEN_ENV).unwrap_or_default()
    };
    normalize_token(&raw)
}

fn normalize_token(raw: &str) -> Result<String, CliError> {
    let token = raw.trim();
    if token.is_empty() {
        return Err(CliError::InvalidInput(format!(
            "no ACS token: pipe it with --token-stdin or set {ACS_TOKEN_ENV}"
        )));
    }
    Ok(token.to_string())
}

fn print_set_dry_run(ctx: &Ctx, args: &SetArgs) {
    let acs_env = args.acs_environment.as_deref().unwrap_or("production");
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            json!({
                "dry_run": true,
                "action": "set",
                "stack": args.stack,
                "acs_environment": acs_env,
                "token_source": if args.token_stdin { "stdin" } else { ACS_TOKEN_ENV },
            })
        ),
        LogFormat::Human => print!(
            "dry-run: would store the ACS credential for stack {} ({acs_env})\npass --yes to apply\n",
            args.stack
        ),
    }
}

fn format_credentials_human(credentials: &[AcsCredential]) -> String {
    if credentials.is_empty() {
        return "No ACS stack credentials. Add one with `deslicer acs credentials set <stack> --token-stdin --yes`.\n".to_string();
    }
    let mut lines = vec!["STACK  ACS_ENV  TOKEN  UPDATED".to_string()];
    for cred in credentials {
        lines.push(format!(
            "{}  {}  {}  {}",
            cred.stack_name,
            cred.environment.as_deref().unwrap_or("-"),
            if cred.token_configured {
                "set"
            } else {
                "missing"
            },
            cred.updated_at.as_deref().unwrap_or("-")
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
        let text = format_credentials_human(&[AcsCredential {
            id: None,
            stack_name: "prod".into(),
            environment: Some("production".into()),
            token_configured: true,
            created_at: None,
            updated_at: Some("2026-10-29T10:00:00Z".into()),
        }]);
        assert!(text.contains("prod"));
        assert!(text.contains("production"));
        assert!(text.contains("set"));
    }

    #[test]
    fn empty_token_is_rejected() {
        assert!(normalize_token("  \n").is_err());
        assert_eq!(normalize_token(" abc \n").unwrap(), "abc");
    }

    #[test]
    fn parses_observer_credential_shape() {
        let cred: AcsCredential = serde_json::from_str(
            r#"{"id":"0b8f2f0e-0000-0000-0000-000000000000","stack_name":"acme","environment":"staging","created_at":"x","updated_at":"y","token_configured":true}"#,
        )
        .unwrap();
        assert_eq!(cred.stack_name, "acme");
        assert!(cred.token_configured);
    }
}
