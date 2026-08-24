use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::Ctx;

use super::session::require_repo_session;

#[derive(ClapArgs)]
pub struct Args {
    /// GitHub App installation id
    #[arg(long)]
    pub installation: i64,

    /// GitHub repository id from `deslicer repo status`
    #[arg(long)]
    pub repo_id: i64,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    if args.installation <= 0 || args.repo_id <= 0 {
        return Err(CliError::Other(
            "--installation and --repo-id must be positive GitHub ids".into(),
        ));
    }
    let (session, client) = authenticate(&ctx, None, None).await?;
    require_repo_session(&session)?;
    client
        .refresh_repo_workflows(args.installation, args.repo_id)
        .await?;
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            serde_json::json!({
                "installation": args.installation,
                "repo_id": args.repo_id,
                "enqueued": true,
            })
        ),
        LogFormat::Human => {
            println!(
                "Enqueued a workflow refresh pull request for repo-id {}.",
                args.repo_id
            );
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn help_has_no_secrets_or_allowlist_regexes() {
        let mut cmd = crate::cli::Cli::command();
        let repo = cmd.find_subcommand_mut("repo").expect("repo");
        let refresh = repo.find_subcommand_mut("refresh").expect("refresh");
        let mut buf = Vec::new();
        refresh.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(!help.contains("dsle_enroll_"));
        assert!(!help.contains("9000000001"));
        assert!(!help.contains("[^/]"));
        assert!(help.contains("--repo-id"));
    }
}
