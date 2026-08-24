use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::ListReposResponse;
use crate::Ctx;

use super::session::require_repo_session;

#[derive(ClapArgs)]
pub struct Args {
    /// GitHub App installation id
    #[arg(long)]
    pub installation: i64,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    if args.installation <= 0 {
        return Err(CliError::Other(
            "--installation must be a positive GitHub App installation id".into(),
        ));
    }
    let (session, client) = authenticate(&ctx, None, None).await?;
    require_repo_session(&session)?;
    let listed = client.list_github_repos(args.installation).await?;
    print_status(&ctx, args.installation, &listed);
    Ok(0)
}

fn print_status(ctx: &Ctx, installation: i64, listed: &ListReposResponse) {
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            serde_json::json!({
                "installation": installation,
                "embedded_workflow_template_digest": listed.embedded_workflow_template_digest,
                "repos": listed.repos,
            })
        ),
        LogFormat::Human => {
            println!("installation {installation}");
            println!(
                "embedded workflow digest {}",
                listed.embedded_workflow_template_digest
            );
            if listed.repos.is_empty() {
                println!("No linked repositories.");
                return;
            }
            for repo in &listed.repos {
                let state = repo.bootstrap_pr_state.as_deref().unwrap_or("-");
                let in_sync = match repo.workflows_in_sync {
                    Some(true) => "yes",
                    Some(false) => "no",
                    None => "-",
                };
                let pending = if repo.workflow_refresh_pending {
                    "pending"
                } else {
                    "idle"
                };
                println!(
                    "{}  repo-id {}  bootstrap {}  in-sync {}  refresh {}",
                    repo.github_full_name, repo.github_repo_id, state, in_sync, pending
                );
                if let Some(url) = &repo.bootstrap_pr_url {
                    println!("  {url}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn help_has_no_secrets_or_allowlist_regexes() {
        let mut cmd = crate::cli::Cli::command();
        let repo = cmd.find_subcommand_mut("repo").expect("repo");
        let status = repo.find_subcommand_mut("status").expect("status");
        let mut buf = Vec::new();
        status.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(!help.contains("dsle_enroll_"));
        assert!(!help.contains("9000000001"));
        assert!(!help.contains("[^/]"));
        assert!(help.contains("--installation"));
    }
}
