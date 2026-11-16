use std::path::PathBuf;

use clap::Args as ClapArgs;
use uuid::Uuid;

use serde_json::{json, Value};

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::Ctx;

mod bind;
mod environment;
mod github_env_recipe;
mod provider;
mod templates;
mod write;

pub use provider::InitProvider;

use bind::{bind_next_step, bind_repo, print_bind_outcome};
use environment::{
    print_environment_write, should_write_environment, write_tenant_environment, EnvironmentWrite,
};
use provider::{detect_provider, origin_for_dir, OriginRepo};
use templates::load_templates;
use write::{print_write_summary, write_templates};

const INIT_EXAMPLES: &str = "\
Examples:
  deslicer init
  deslicer init --provider github
  deslicer init --provider github-token --environment acme-prod
  deslicer init --provider github --bind --environment prod --target-group <uuid>

Path A2 (Observer token, no GitHub App) writes `.deslicer/environments/<stem>.yml`
and prints a GitHub Environment setup recipe (not executed):
  deslicer init --provider github-token --environment acme-prod
  deslicer inventory sync
  deslicer docs path-a2
";

#[derive(ClapArgs)]
#[command(after_long_help = INIT_EXAMPLES)]
pub struct Args {
    /// github, github-token, gitlab, bitbucket, azure, or auto (from `git remote get-url origin`)
    #[arg(long, default_value = "auto")]
    pub provider: String,

    /// Tenant slug; resolves to the environment file stem and GitHub Environment name.
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

    /// Overwrite existing workflow / pipeline files (not environment `apps:` lists).
    #[arg(long)]
    pub force: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let log_format = ctx.log_format;
    match run_inner(ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(log_format, err),
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
    let environment = maybe_write_environment(
        &ctx,
        &args,
        provider,
        session.as_ref(),
        client.as_ref(),
        &dir,
    )
    .await?;

    if !args.bind {
        print_init_result(
            &ctx,
            provider,
            &dir,
            summary.written,
            summary.skipped,
            environment.as_ref(),
            origin.as_ref(),
        );
        return Ok(0);
    }
    if !matches!(ctx.log_format, LogFormat::Json) {
        print_write_summary(&ctx, provider, &dir, summary.written, summary.skipped);
        if let Some(written) = environment.as_ref() {
            print_environment_write(&ctx, written, origin.as_ref());
        }
    }

    let session = session
        .as_ref()
        .ok_or_else(|| CliError::Other("--bind cannot be used with --offline".into()))?;
    let client = client
        .as_ref()
        .ok_or_else(|| CliError::Other("--bind cannot be used with --offline".into()))?;
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

async fn maybe_write_environment(
    ctx: &Ctx,
    args: &Args,
    provider: InitProvider,
    session: Option<&crate::commands::pipeline::AuthenticatedSession>,
    client: Option<&crate::observer_client::Client>,
    dir: &std::path::Path,
) -> Result<Option<EnvironmentWrite>, CliError> {
    if !should_write_environment(provider, session) {
        return Ok(None);
    }
    if args.offline {
        if !matches!(ctx.log_format, LogFormat::Json) {
            println!("Skipped tenant environment file (--offline; cannot reach Observer).");
        }
        return Ok(None);
    }
    let Some(client) = client else {
        if !matches!(ctx.log_format, LogFormat::Json) {
            println!("Skipped tenant environment file (Observer client unavailable).");
        }
        return Ok(None);
    };
    let written = write_tenant_environment(dir, client, args.environment.as_deref()).await?;
    Ok(Some(written))
}

fn print_init_result(
    ctx: &Ctx,
    provider: InitProvider,
    dir: &std::path::Path,
    written: usize,
    skipped: usize,
    environment: Option<&EnvironmentWrite>,
    origin: Option<&OriginRepo>,
) {
    let heading = if matches!(provider, InitProvider::GithubToken) {
        "Next steps:"
    } else {
        "Bind this repo (optional):"
    };
    let mut steps = vec![heading.to_string(), bind_next_step(provider)];
    if let Some(environment) = environment {
        if matches!(provider, InitProvider::GithubToken) {
            let repo = github_env_recipe::origin_repo_slug(origin);
            steps.push(github_env_recipe::github_environment_recipe(
                &environment.stem,
                repo.as_deref(),
            ));
        }
    }
    match ctx.log_format {
        LogFormat::Json => {
            let mut doc = json!({
                "schema_version": 1,
                "provider": provider.as_str(),
                "dir": dir.display().to_string(),
                "written": written,
                "skipped": skipped,
                "next_steps": steps,
            });
            if let Some(environment) = environment {
                if let Some(obj) = doc.as_object_mut() {
                    obj.insert(
                        "wrote".into(),
                        Value::String(environment.relative_path.clone()),
                    );
                    obj.insert(
                        "github_environment".into(),
                        Value::String(environment.stem.clone()),
                    );
                }
            }
            let _ = crate::json_doc::print_success(&doc);
        }
        LogFormat::Human => {
            print_write_summary(ctx, provider, dir, written, skipped);
            if let Some(environment) = environment {
                print_environment_write(ctx, environment, origin);
            }
            println!();
            println!("{heading}");
            println!("{}", bind_next_step(provider));
        }
    }
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
