use std::path::PathBuf;

use clap::Args as ClapArgs;
use uuid::Uuid;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::Ctx;

mod bind;
mod provider;
mod templates;
mod write;

pub use provider::InitProvider;

use bind::{bind_next_step, bind_repo, BindOutcome};
use provider::{detect_provider, origin_for_dir, OriginRepo};
use templates::load_templates;
use write::write_templates;

#[derive(ClapArgs)]
pub struct Args {
    /// github, github-token, gitlab, bitbucket, azure, or auto (from `git remote get-url origin`)
    #[arg(long, default_value = "auto")]
    pub provider: String,

    #[arg(long)]
    pub environment: Option<String>,

    /// Host group UUID. Required with --bind.
    #[arg(long)]
    pub target_group: Option<String>,

    /// Repository root to write into (defaults to the current directory).
    #[arg(long)]
    pub dir: Option<PathBuf>,

    /// Create the environment binding after writing files.
    #[arg(long)]
    pub bind: bool,

    /// Use the last fetched template cache; fail if it is missing.
    #[arg(long)]
    pub offline: bool,

    /// Overwrite existing workflow / pipeline files.
    #[arg(long)]
    pub force: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: Ctx, args: Args) -> Result<i32, CliError> {
    if args.bind && (args.environment.is_none() || args.target_group.is_none()) {
        return Err(CliError::Other(
            "--bind requires --environment and --target-group".into(),
        ));
    }
    let dir = args
        .dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    if !dir.is_dir() {
        return Err(CliError::Other(format!(
            "--dir {} is not a directory",
            dir.display()
        )));
    }

    let (provider, origin) = resolve_provider(&args.provider, &dir)?;
    let (session, client) = if args.offline {
        (None, None)
    } else {
        let pair = authenticate(&ctx, None, None).await?;
        (Some(pair.0), Some(pair.1))
    };

    let files = load_templates(client.as_ref(), provider, args.offline).await?;
    let summary = write_templates(&dir, provider, &files, args.force)?;
    print_write_summary(&ctx, provider, &dir, summary.written, summary.skipped);

    if !args.bind {
        println!();
        if matches!(provider, InitProvider::GithubToken) {
            println!("Next steps:");
        } else {
            println!("Bind this repo (optional):");
        }
        println!("{}", bind_next_step(provider));
        return Ok(0);
    }

    let Some(session) = session.as_ref() else {
        return Err(CliError::Other(
            "--bind cannot be used with --offline".into(),
        ));
    };
    let Some(client) = client.as_ref() else {
        return Err(CliError::Other(
            "--bind cannot be used with --offline".into(),
        ));
    };
    let origin = origin_or_detect(origin, &dir)?;
    let environment = args.environment.as_deref().unwrap_or("");
    let target_group = parse_target_group(args.target_group.as_deref())?;
    let outcome = bind_repo(
        client,
        session,
        provider,
        &origin,
        environment,
        target_group,
    )
    .await?;
    print_bind_outcome(&ctx, &outcome);
    Ok(0)
}

fn resolve_provider(
    raw: &str,
    dir: &std::path::Path,
) -> Result<(InitProvider, Option<OriginRepo>), CliError> {
    if raw.eq_ignore_ascii_case("auto") {
        let (provider, origin) = detect_provider(dir)?;
        return Ok((provider, Some(origin)));
    }
    Ok((InitProvider::parse(raw)?, None))
}

fn origin_or_detect(
    origin: Option<OriginRepo>,
    dir: &std::path::Path,
) -> Result<OriginRepo, CliError> {
    match origin {
        Some(origin) => Ok(origin),
        None => origin_for_dir(dir),
    }
}

fn parse_target_group(raw: Option<&str>) -> Result<Uuid, CliError> {
    let Some(raw) = raw else {
        return Err(CliError::Other(
            "--target-group is required with --bind".into(),
        ));
    };
    Uuid::parse_str(raw.trim()).map_err(|_| CliError::Other("--target-group must be a UUID".into()))
}

fn print_write_summary(
    ctx: &Ctx,
    provider: InitProvider,
    dir: &std::path::Path,
    written: usize,
    skipped: usize,
) {
    match ctx.log_format {
        LogFormat::Json => {
            let payload = serde_json::json!({
                "provider": provider.as_str(),
                "dir": dir.display().to_string(),
                "written": written,
                "skipped": skipped,
            });
            println!("{payload}");
        }
        LogFormat::Human => {
            println!(
                "Wrote {written} {} file(s) under {}.",
                provider.as_str(),
                dir.display()
            );
            if skipped > 0 {
                println!("Skipped {skipped} existing README file(s) (IfMissing).");
            }
        }
    }
}

fn print_bind_outcome(ctx: &Ctx, outcome: &BindOutcome) {
    match outcome {
        BindOutcome::Bound { already } => {
            if matches!(ctx.log_format, LogFormat::Json) {
                println!(
                    "{}",
                    serde_json::json!({ "bound": true, "already": already })
                );
                return;
            }
            if *already {
                println!("Environment already bound.");
            } else {
                println!("Environment binding created.");
            }
        }
        BindOutcome::NeedsGithubConnect => {
            println!("No GitHub App installation covers this org.");
            println!("Connect GitHub in the portal: Platform → GitHub → Connect");
            println!("Files were still written.");
        }
        BindOutcome::PrintPortal { message } => {
            println!("{message}");
        }
    }
}
