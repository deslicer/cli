use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error, require_proxy_mode};
use crate::errors::CliError;
use crate::observer_client::{ChangePlan, Client, OrchestratedPlan};
use crate::output::emit_change_plan;
use crate::token_source::TokenSource;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,

    /// Return immediately after the compile-runner is triggered instead of
    /// waiting for the plan to reach pending_approval.
    #[arg(long)]
    pub no_wait: bool,

    /// GitHub-App-free flow: package this directory into a digest-pinned
    /// bundle, upload it, and compile the plan from it instead of a git clone.
    /// Requires OBSERVER_API_URL and DESLICER_API_TOKEN (direct mode).
    #[arg(long, requires = "target_group")]
    pub source_dir: Option<std::path::PathBuf>,

    /// Host group UUID the bundle-sourced plan targets (required with
    /// --source-dir).
    #[arg(long)]
    pub target_group: Option<String>,

    /// Optional plan name for the bundle-sourced plan.
    #[arg(long)]
    pub name: Option<String>,
}

/// Compile polling: the ephemeral compile-runner takes seconds to a few
/// minutes to clone, parse, diff, and post the plan draft.
const COMPILE_POLL_ATTEMPTS: u32 = 60;
const COMPILE_POLL_INTERVAL_SECS: u64 = 5;

fn is_still_compiling(status: &str) -> bool {
    matches!(status, "draft" | "compiling" | "compile_pending")
}

fn is_compile_failure(status: &str) -> bool {
    matches!(status, "failed" | "compile_failed" | "rejected")
}

async fn wait_for_compile(client: &Client, plan_id: &str) -> Result<ChangePlan, String> {
    let mut last_err: Option<String> = None;
    for _ in 0..COMPILE_POLL_ATTEMPTS {
        match client.get_plan(plan_id).await {
            Ok(plan) if !is_still_compiling(&plan.status) => return Ok(plan),
            Ok(_) => last_err = None,
            // The draft row may not be visible yet right after creation.
            Err(err) => last_err = Some(err.to_string()),
        }
        tokio::time::sleep(std::time::Duration::from_secs(COMPILE_POLL_INTERVAL_SECS)).await;
    }
    Err(last_err.unwrap_or_else(|| {
        format!(
            "plan {plan_id} did not finish compiling within {}s",
            u64::from(COMPILE_POLL_ATTEMPTS) * COMPILE_POLL_INTERVAL_SECS
        )
    }))
}

fn orchestrated_as_change_plan(created: &OrchestratedPlan) -> ChangePlan {
    ChangePlan {
        id: created.plan_row_id.clone().unwrap_or_default(),
        plan_id: Some(created.plan_id.clone()),
        status: created.status.clone(),
        name: None,
        summary: None,
    }
}

/// Direct-mode client for the bundle flow: static Observer API key, no CI
/// OIDC and no proxy (the whole point of the GitHub-App-free path).
fn bundle_flow_client(ctx: &Ctx) -> Result<Client, CliError> {
    let base = ctx.observer_api_url.clone().ok_or_else(|| {
        CliError::Other(
            "--source-dir requires direct Observer access: set --observer-api-url \
             or the OBSERVER_API_URL env var"
                .into(),
        )
    })?;
    let token = std::env::var("DESLICER_API_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| {
            CliError::Other(
                "--source-dir requires the DESLICER_API_TOKEN env var (an Observer \
                 API key with the `tools` scope)"
                    .into(),
            )
        })?;
    Ok(Client::new(base, TokenSource::static_token(token)))
}

async fn run_bundle_flow(ctx: &Ctx, args: &Args) -> Result<ChangePlan, CliError> {
    let source_dir = args.source_dir.as_deref().expect("clap guarantees value");
    let target_group = args.target_group.as_deref().expect("clap requires flag");

    let client = bundle_flow_client(ctx)?;

    let packaged = crate::bundle::package_directory(source_dir)?;
    eprintln!(
        "packaged {} files ({} bytes, sha256 {})",
        packaged.file_count,
        packaged.bytes.len(),
        packaged.sha256
    );

    let label = source_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    let uploaded = client
        .upload_bundle(packaged.bytes, &packaged.sha256, label.as_deref())
        .await?;
    eprintln!("bundle uploaded: {}", uploaded.id);

    let plan = client
        .create_plan_from_bundle(&uploaded.id, target_group, args.name.as_deref())
        .await?;

    // Bundle plans carry no git ref; the source identity is the digest.
    client.trigger_compile(&plan.id, "bundle").await?;

    if args.no_wait {
        return Ok(plan);
    }

    let compiled = wait_for_compile(&client, plan.external_id())
        .await
        .map_err(CliError::Other)?;
    if is_compile_failure(&compiled.status) {
        return Err(CliError::Other(format!(
            "plan compile failed with status: {}",
            compiled.status
        )));
    }
    Ok(compiled)
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    if args.source_dir.is_some() {
        return match run_bundle_flow(&ctx, &args).await {
            Ok(plan) => emit_change_plan(&plan),
            Err(err) => map_cli_error(err),
        };
    }

    let (session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(err),
    };

    if let Err(err) = require_proxy_mode(&session, "change plan") {
        return map_cli_error(err);
    }

    let created = match client
        .create_plan_orchestrated(args.environment.as_deref())
        .await
    {
        Ok(created) => created,
        Err(err) => return map_cli_error(err),
    };

    // Older proxy builds return only the internal row id, which cannot be
    // polled through GET /plans/{plan_id} — skip waiting in that case.
    if args.no_wait || created.plan_row_id.is_none() {
        return emit_change_plan(&orchestrated_as_change_plan(&created));
    }

    let plan = match wait_for_compile(&client, &created.plan_id).await {
        Ok(plan) => plan,
        Err(msg) => {
            eprintln!("plan compile did not complete: {msg}");
            emit_change_plan(&orchestrated_as_change_plan(&created));
            return 1;
        }
    };

    if is_compile_failure(&plan.status) {
        eprintln!("plan compile failed with status: {}", plan.status);
        emit_change_plan(&plan);
        return 1;
    }

    emit_change_plan(&plan)
}
