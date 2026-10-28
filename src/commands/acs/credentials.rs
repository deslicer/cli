use clap::{Args as ClapArgs, Subcommand};

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::SplunkCredential;
use crate::Ctx;

#[derive(Subcommand)]
pub enum CredentialsCmd {
    /// List Splunk Cloud credentials stored for ACS
    List(ListArgs),
    /// Test that a named credential can reach Splunk Cloud ACS
    Test(TestArgs),
}

#[derive(ClapArgs)]
pub struct ListArgs {
    #[arg(long)]
    pub environment: Option<String>,
}

#[derive(ClapArgs)]
pub struct TestArgs {
    /// Credential name as shown by `acs credentials list`
    #[arg(long)]
    pub name: String,

    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn dispatch(ctx: Ctx, cmd: CredentialsCmd) -> i32 {
    match cmd {
        CredentialsCmd::List(args) => list(ctx, args).await,
        CredentialsCmd::Test(args) => test(ctx, args).await,
    }
}

async fn list(ctx: Ctx, args: ListArgs) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    let credentials = match client.list_splunk_credentials().await {
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

    match ctx.log_format {
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
                eprintln!("failed to serialize test result: {err}");
                1
            }
        },
        LogFormat::Human => {
            let status = if result.ok { "ok" } else { "failed" };
            let detail = result.message.as_deref().unwrap_or("");
            if detail.is_empty() {
                println!("credential {}: {status}", args.name);
            } else {
                println!("credential {}: {status} — {detail}", args.name);
            }
            if result.ok {
                0
            } else {
                1
            }
        }
    }
}

fn format_credentials_human(credentials: &[SplunkCredential]) -> String {
    if credentials.is_empty() {
        return "No Splunk Cloud credentials. Add them in the portal, then rerun.\n".to_string();
    }
    let mut lines = vec!["NAME  KIND  STACK  STATUS".to_string()];
    for cred in credentials {
        lines.push(format!(
            "{}  {}  {}  {}",
            cred.name,
            cred.kind.as_deref().unwrap_or("-"),
            cred.stack.as_deref().unwrap_or("-"),
            cred.status.as_deref().unwrap_or("-")
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
        let text = format_credentials_human(&[SplunkCredential {
            name: "prod-token".into(),
            kind: Some("acs".into()),
            stack: Some("prod".into()),
            status: Some("valid".into()),
        }]);
        assert!(text.contains("prod-token"));
        assert!(text.contains("acs"));
    }
}
