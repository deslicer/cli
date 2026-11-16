use std::path::PathBuf;

use clap::Args as ClapArgs;
use serde_json::json;

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
    pub name: Option<String>,

    /// Optional GitHub repository description
    #[arg(long)]
    pub description: Option<String>,

    /// Create the repository. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,

    /// Read-only GitHub App bootstrap readiness (no create / no secret writes)
    #[arg(long)]
    pub check: bool,

    /// Focus readiness on one linked github_repo_id (with `--check`)
    #[arg(long)]
    pub repo_id: Option<i64>,

    /// Optional local checkout to inspect Path A OIDC workflow permissions
    #[arg(long)]
    pub dir: Option<PathBuf>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let log_format = ctx.log_format;
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(log_format, err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    validate_args(&args)?;
    let (session, client) = authenticate(&ctx, None, None).await?;
    require_repo_session(&session)?;

    if args.check {
        return run_check(&ctx, &client, &args).await;
    }

    let name = args
        .name
        .as_deref()
        .ok_or_else(|| CliError::Other("--name is required unless --check is set".into()))?;
    let org = installation_org(&client, args.installation).await?;
    if !args.yes {
        print_dry_run(&ctx, &org, name);
        return Ok(0);
    }

    let created = client
        .provision_github_repo(
            args.installation,
            &ProvisionRepoRequest {
                repo_name: name.to_string(),
                visibility: "private".into(),
                description: args.description.clone(),
            },
        )
        .await?;
    print_created(&ctx, &created);
    Ok(0)
}

async fn run_check(ctx: &Ctx, client: &Client, args: &Args) -> Result<i32, CliError> {
    let report = super::readiness::collect(
        client,
        super::readiness::Query {
            installation: args.installation,
            name: args.name.clone(),
            repo_id: args.repo_id,
            dir: args.dir.clone(),
        },
    )
    .await?;
    let ok = report.checks.iter().all(|check| check.ok);
    print_check_results(
        ctx,
        args.installation,
        report.org.as_deref(),
        &report.checks,
        ok,
    );
    Ok(if ok { 0 } else { 1 })
}

fn print_check_results(
    ctx: &Ctx,
    installation: i64,
    org: Option<&str>,
    checks: &[super::readiness::CheckItem],
    ok: bool,
) {
    match ctx.log_format {
        LogFormat::Json => {
            let _ = crate::json_doc::print_success(&json!({
                "ok": ok,
                "installation": installation,
                "org": org,
                "checks": checks.iter().map(|c| json!({
                    "id": c.id,
                    "ok": c.ok,
                    "detail": c.detail,
                })).collect::<Vec<_>>(),
            }));
        }
        LogFormat::Human => {
            println!(
                "repo bootstrap --check installation {installation}{}",
                org.map(|o| format!(" org {o}")).unwrap_or_default()
            );
            for check in checks {
                let mark = if check.ok { "ok" } else { "FAIL" };
                println!("[{mark}] {}: {}", check.id, check.detail);
            }
            if ok {
                println!("ready: GitHub App bootstrap surface looks usable");
            } else {
                println!("not ready: fix FAIL checks above before workshop dry-run");
            }
        }
    }
}

fn validate_args(args: &Args) -> Result<(), CliError> {
    if args.installation <= 0 {
        return Err(CliError::InvalidInput(
            "--installation must be a positive GitHub App installation id".into(),
        ));
    }
    if args.check && args.yes {
        return Err(CliError::InvalidInput(
            "--check and --yes are mutually exclusive".into(),
        ));
    }
    if !args.check {
        let name = args.name.as_deref().ok_or_else(|| {
            CliError::InvalidInput("--name is required unless --check is set".into())
        })?;
        if !valid_repo_name(name) {
            return Err(CliError::InvalidInput(
                "--name must be letters, digits, dots, hyphens, or underscores (1–100; not '.' or '..')"
                    .into(),
            ));
        }
    } else if let Some(name) = args.name.as_deref() {
        // Allow org/name or short name for focus matching.
        if name.is_empty() {
            return Err(CliError::InvalidInput("--name must not be empty".into()));
        }
    }
    if let Some(description) = args.description.as_deref() {
        validate_description(description)?;
    }
    if let Some(repo_id) = args.repo_id {
        if !args.check {
            return Err(CliError::InvalidInput(
                "--repo-id is only valid with --check".into(),
            ));
        }
        if repo_id <= 0 {
            return Err(CliError::InvalidInput("--repo-id must be positive".into()));
        }
    }
    if args.dir.is_some() && !args.check {
        return Err(CliError::InvalidInput(
            "--dir is only valid with --check".into(),
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

pub(crate) fn valid_repo_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 100 || name == "." || name == ".." {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn validate_description(description: &str) -> Result<(), CliError> {
    if description.len() > MAX_DESCRIPTION_LEN || description.chars().any(char::is_control) {
        return Err(CliError::InvalidInput(
            "--description must be at most 350 characters with no control characters".into(),
        ));
    }
    Ok(())
}

pub(crate) fn dry_run_text(org: &str, name: &str) -> String {
    format!("org: {org}\nname: {name}\nvisibility: private\nPass --yes to create this private repository.")
}

fn print_dry_run(ctx: &Ctx, org: &str, name: &str) {
    match ctx.log_format {
        LogFormat::Json => {
            let _ = crate::json_doc::print_success(&serde_json::json!({
                "dry_run": true,
                "org": org,
                "name": name,
                "visibility": "private",
            }));
        }
        LogFormat::Human => println!("{}", dry_run_text(org, name)),
    }
}

fn print_created(ctx: &Ctx, created: &ProvisionRepoResponse) {
    match ctx.log_format {
        LogFormat::Json => {
            let _ = crate::json_doc::print_success(&serde_json::json!({
                "repo_full_name": created.repo_full_name,
                "html_url": created.html_url,
                "github_repo_id": created.github_repo_id,
                "default_branch": created.default_branch,
            }));
        }
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
    fn check_rejects_yes_and_requires_name_otherwise() {
        let err = validate_args(&Args {
            installation: 1,
            name: Some("repo".into()),
            description: None,
            yes: true,
            check: true,
            repo_id: None,
            dir: None,
        })
        .expect_err("mutex");
        assert!(err.to_string().contains("mutually exclusive"));
        assert_eq!(err.exit_code(), 2);

        let err = validate_args(&Args {
            installation: 1,
            name: None,
            description: None,
            yes: false,
            check: false,
            repo_id: None,
            dir: None,
        })
        .expect_err("name");
        assert!(err.to_string().contains("--name is required"));
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn installation_zero_is_usage_exit_2() {
        let err = validate_args(&Args {
            installation: 0,
            name: None,
            description: None,
            yes: false,
            check: true,
            repo_id: None,
            dir: None,
        })
        .expect_err("installation");
        assert!(err.to_string().contains("positive"));
        assert_eq!(err.exit_code(), 2);
        assert!(matches!(err, CliError::InvalidInput(_)));
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
        assert!(help.contains("--check"));
        assert!(help.contains("--installation"));
    }
}
