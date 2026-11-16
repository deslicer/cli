//! `deslicer doctor` — readiness and API URL drift in one place (#187). // pragma: allowlist secret
//!
//! Local checks never call the network. Pass `--installation` to also run the
//! live GitHub App readiness checks from `repo bootstrap --check`.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::commands::repo::readiness::{self, CheckItem};
use crate::errors::CliError;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Local checkout whose `.github/workflows` should be inspected.
    #[arg(long)]
    pub dir: Option<PathBuf>,

    /// GitHub App installation id. When set, run live `repo bootstrap --check`.
    #[arg(long)]
    pub installation: Option<i64>,

    /// Focus the live check on one linked repository name (`org/name` or short).
    #[arg(long)]
    pub name: Option<String>,

    /// Focus the live check on one linked github_repo_id.
    #[arg(long)]
    pub repo_id: Option<i64>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let format = ctx.log_format;
    match run_inner(&ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(format, err),
    }
}

async fn run_inner(ctx: &Ctx, args: Args) -> Result<i32, CliError> {
    validate(&args)?;
    let mut checks = vec![api_url_check(ctx)];

    if let Some(installation) = args.installation {
        let (_session, client) = authenticate(ctx, None, None).await?;
        let report = readiness::collect(
            &client,
            readiness::Query {
                installation,
                name: args.name.clone(),
                repo_id: args.repo_id,
                dir: args.dir.clone(),
            },
        )
        .await?;
        checks.extend(report.checks);
    } else {
        checks.push(local_workflow_check(args.dir.as_deref()));
        checks.push(manual_secrets_check());
    }

    let ok = checks.iter().all(|check| check.ok);
    print_report(ctx, &checks, ok, args.installation);
    Ok(if ok { 0 } else { 1 })
}

fn validate(args: &Args) -> Result<(), CliError> {
    if let Some(installation) = args.installation {
        if installation <= 0 {
            return Err(CliError::InvalidInput(
                "--installation must be a positive GitHub App installation id".into(),
            ));
        }
    } else if args.repo_id.is_some() || args.name.is_some() {
        return Err(CliError::InvalidInput(
            "--name and --repo-id require --installation".into(),
        ));
    }
    if let Some(repo_id) = args.repo_id {
        if repo_id <= 0 {
            return Err(CliError::InvalidInput("--repo-id must be positive".into()));
        }
    }
    Ok(())
}

fn api_url_check(ctx: &Ctx) -> CheckItem {
    match crate::token_store::load_stored_session() {
        Ok(Some(session)) => match crate::commands::auth::status::api_url_drift(ctx, &session) {
            Some(detail) => CheckItem {
                id: "api_url",
                ok: false,
                detail,
            },
            None => CheckItem {
                id: "api_url",
                ok: true,
                detail: "stored session portal matches the CLI API URL".into(),
            },
        },
        Ok(None) => CheckItem {
            id: "api_url",
            ok: true,
            detail:
                "no stored device session; API URL drift is checked after `deslicer auth login`" // pragma: allowlist secret
                    .into(),
        },
        Err(err) => CheckItem {
            id: "api_url",
            ok: false,
            detail: format!("could not read the stored session: {err}"),
        },
    }
}

fn local_workflow_check(dir: Option<&std::path::Path>) -> CheckItem {
    match dir {
        Some(dir) => readiness::local_oidc_workflow_check(dir),
        None => CheckItem {
            id: "local_oidc_workflows",
            ok: true,
            detail: "skipped (pass --dir <checkout> to inspect Path A id-token permissions)".into(),
        },
    }
}

fn manual_secrets_check() -> CheckItem {
    CheckItem {
        id: "secrets_and_push",
        ok: true,
        detail: "manual: CLI does not write GitHub Environment secrets or push git content; \
                 live installation readiness is `deslicer repo bootstrap --check --installation <id>`" // pragma: allowlist secret
            .into(),
    }
}

fn print_report(ctx: &Ctx, checks: &[CheckItem], ok: bool, installation: Option<i64>) {
    let next_command = if ok {
        None
    } else if checks.iter().any(|c| c.id == "api_url" && !c.ok) {
        Some("deslicer auth status") // pragma: allowlist secret
    } else {
        None
    };
    match ctx.log_format {
        LogFormat::Json => {
            let _ = crate::json_doc::print_success(&json!({
                "ok": ok,
                "installation": installation,
                "checks": checks.iter().map(CheckItem::to_json).collect::<Vec<_>>(),
                "next_command": next_command,
            }));
        }
        LogFormat::Human => {
            println!("deslicer doctor"); // pragma: allowlist secret
            for check in checks {
                let mark = if check.ok { "ok" } else { "FAIL" };
                println!("[{mark}] {}: {}", check.id, check.detail);
            }
            if ok {
                println!("ready");
            } else {
                println!("not ready");
                if let Some(next) = next_command {
                    println!("next: {next}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_focus_without_installation_is_usage() {
        let err = validate(&Args {
            dir: None,
            installation: None,
            name: Some("acme/repo".into()),
            repo_id: None,
        })
        .expect_err("name");
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn zero_installation_is_usage() {
        let err = validate(&Args {
            dir: None,
            installation: Some(0),
            name: None,
            repo_id: None,
        })
        .expect_err("installation");
        assert!(err.to_string().contains("positive"));
    }
}
