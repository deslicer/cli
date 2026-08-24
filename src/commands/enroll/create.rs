use std::io::IsTerminal;
use std::path::PathBuf;

use clap::Args as ClapArgs;
use uuid::Uuid;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::CreateEnrollmentTokenRequest;
use crate::Ctx;

use super::write_token::{require_write_file_if_not_tty, write_token_file};

#[derive(ClapArgs)]
pub struct Args {
    /// insights (monitoring) or bootstrap (worker)
    #[arg(long, value_parser = ["insights", "bootstrap"])]
    pub purpose: String,

    #[arg(long)]
    pub name: Option<String>,

    #[arg(long)]
    pub max_hosts: Option<u32>,

    #[arg(long)]
    pub expires_days: Option<u32>,

    /// Optional hosts.id UUID. Omit for a fleet token.
    #[arg(long)]
    pub bind_host: Option<Uuid>,

    /// Write the one-time token to PATH (0600, must not exist). Required when stdout is not a TTY.
    #[arg(long)]
    pub write_file: Option<PathBuf>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    let is_tty = std::io::stdout().is_terminal();
    require_write_file_if_not_tty(is_tty, args.write_file.as_deref())?;

    let (session, client) = authenticate(&ctx, None, None).await?;
    if !session.is_device_session() {
        return Err(CliError::Other(
            "`enroll create` requires `deslicer auth login` (device session)".into(),
        ));
    }

    let created = client
        .create_enrollment_token(&CreateEnrollmentTokenRequest {
            name: args.name.clone(),
            max_hosts: args.max_hosts,
            expires_in_days: args.expires_days,
            bind_to_host_id: args.bind_host,
            token_type: args.purpose.clone(),
        })
        .await?;

    if let Some(path) = args.write_file.as_deref() {
        write_token_file(path, &created.token)?;
    }

    print_created(&ctx, &created, args.write_file.as_deref());
    Ok(0)
}

fn print_created(
    ctx: &Ctx,
    created: &crate::observer_client::CreateEnrollmentTokenResponse,
    write_file: Option<&std::path::Path>,
) {
    match ctx.log_format {
        LogFormat::Json => {
            let mut payload = serde_json::json!({
                "jti": created.jti,
                "expires_at": created.expires_at,
                "max_hosts": created.max_hosts,
                "bind_to_host_id": created.bind_to_host_id,
            });
            if write_file.is_none() {
                payload["token"] = serde_json::Value::String(created.token.clone());
            }
            println!("{payload}");
        }
        LogFormat::Human => {
            if let Some(path) = write_file {
                println!(
                    "Wrote enrollment token to {} (shown once; file mode 0600).",
                    path.display()
                );
            } else {
                println!("{}", created.token);
                println!("This token is shown once. Store it, then revoke with:");
            }
            println!("deslicer enroll revoke --jti {}", created.jti);
            if let Some(path) = write_file {
                println!(
                    "Next: deslicer worker instructions --token-file {}",
                    path.display()
                );
            } else {
                println!("Next: deslicer worker instructions");
            }
            println!("Pending Approvals in the portal remain the trust gate.");
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn help_has_no_realistic_token() {
        let mut cmd = crate::cli::Cli::command();
        let enroll = cmd.find_subcommand_mut("enroll").expect("enroll");
        let create = enroll.find_subcommand_mut("create").expect("create");
        let mut buf = Vec::new();
        create.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(!help.contains("dsle_enroll_"));
        assert!(help.contains("--write-file"));
        assert!(help.contains("--purpose"));
    }
}
