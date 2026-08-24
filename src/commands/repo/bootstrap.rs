use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::{
    Client, GithubInstallation, ProvisionRepoRequest, ProvisionRepoResponse,
};
use crate::Ctx;

use super::session::require_repo_session;

const MAX_DESCRIPTION_LEN: usize = 350;

#[derive(ClapArgs)]
pub struct Args {
    /// GitHub App installation id from `deslicer repo status` / the portal
    #[arg(long)]
    pub installation: i64,

    /// New private repository name in the installation organization
    #[arg(long)]
    pub name: String,

    /// Optional GitHub repository description
    #[arg(long)]
    pub description: Option<String>,

    /// Create the repository. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    validate_args(&args)?;
    let (session, client) = authenticate(&ctx, None, None).await?;
    require_repo_session(&session)?;

    let org = installation_org(&client, args.installation).await?;
    if !args.yes {
        print_dry_run(&ctx, &org, &args.name);
        return Ok(0);
    }

    let created = client
        .provision_github_repo(
            args.installation,
            &ProvisionRepoRequest {
                repo_name: args.name.clone(),
                visibility: "private".into(),
                description: args.description.clone(),
            },
        )
        .await?;
    print_created(&ctx, &created);
    Ok(0)
}

fn validate_args(args: &Args) -> Result<(), CliError> {
    if args.installation <= 0 {
        return Err(CliError::Other(
            "--installation must be a positive GitHub App installation id".into(),
        ));
    }
    if !valid_repo_name(&args.name) {
        return Err(CliError::Other(
            "--name must be letters, digits, dots, hyphens, or underscores (1–100; not '.' or '..')"
                .into(),
        ));
    }
    if let Some(description) = args.description.as_deref() {
        validate_description(description)?;
    }
    Ok(())
}

pub(crate) fn valid_repo_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 100 || name == "." || name == ".." {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn validate_description(description: &str) -> Result<(), CliError> {
    if description.len() > MAX_DESCRIPTION_LEN || description.chars().any(char::is_control) {
        return Err(CliError::Other(
            "--description must be at most 350 characters with no control characters".into(),
        ));
    }
    Ok(())
}

async fn installation_org(client: &Client, installation_id: i64) -> Result<String, CliError> {
    let installations = client.list_github_installations().await?;
    find_installation_org(&installations, installation_id)
}

pub(crate) fn find_installation_org(
    installations: &[GithubInstallation],
    installation_id: i64,
) -> Result<String, CliError> {
    installations
        .iter()
        .find(|row| row.installation_id == installation_id && row.status != "deleted")
        .map(|row| row.github_account_login.clone())
        .ok_or_else(|| {
            CliError::Other(
                "GitHub App installation not found. Connect GitHub in the portal (Platform → GitHub)."
                    .into(),
            )
        })
}

pub(crate) fn dry_run_text(org: &str, name: &str) -> String {
    format!("org: {org}\nname: {name}\nvisibility: private\nPass --yes to create this private repository.")
}

fn print_dry_run(ctx: &Ctx, org: &str, name: &str) {
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            serde_json::json!({
                "dry_run": true,
                "org": org,
                "name": name,
                "visibility": "private",
            })
        ),
        LogFormat::Human => println!("{}", dry_run_text(org, name)),
    }
}

fn print_created(ctx: &Ctx, created: &ProvisionRepoResponse) {
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            serde_json::json!({
                "repo_full_name": created.repo_full_name,
                "html_url": created.html_url,
                "github_repo_id": created.github_repo_id,
                "default_branch": created.default_branch,
            })
        ),
        LogFormat::Human => {
            println!(
                "Created private repository {} ({})",
                created.repo_full_name, created.html_url
            );
            println!(
                "repo-id {} default-branch {}",
                created.github_repo_id, created.default_branch
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn rejects_invalid_repo_names() {
        assert!(!valid_repo_name(""));
        assert!(!valid_repo_name("."));
        assert!(!valid_repo_name(".."));
        assert!(!valid_repo_name("has space"));
        assert!(!valid_repo_name(&"a".repeat(101)));
        assert!(valid_repo_name("splunk-config"));
        assert!(valid_repo_name("org.repo_1"));
    }

    #[test]
    fn dry_run_names_org_name_and_private() {
        let text = dry_run_text("acme", "splunk-config");
        assert!(text.contains("org: acme"));
        assert!(text.contains("name: splunk-config"));
        assert!(text.contains("visibility: private"));
        assert!(text.contains("--yes"));
    }

    #[test]
    fn missing_installation_does_not_provision() {
        let err = find_installation_org(&[], 42).expect_err("missing");
        assert!(err.to_string().contains("Platform → GitHub"));
    }

    #[test]
    fn help_has_no_secrets_or_allowlist_regexes() {
        let mut cmd = crate::cli::Cli::command();
        let repo = cmd.find_subcommand_mut("repo").expect("repo");
        let bootstrap = repo.find_subcommand_mut("bootstrap").expect("bootstrap");
        let mut buf = Vec::new();
        bootstrap.write_long_help(&mut buf).expect("help");
        let help = String::from_utf8(buf).expect("utf8");
        assert!(!help.contains("dsle_enroll_"));
        assert!(!help.contains("9000000001"));
        assert!(!help.contains("[^/]"));
        assert!(!help.contains("github-installations"));
        assert!(help.contains("--yes"));
        assert!(help.contains("--installation"));
    }
}
