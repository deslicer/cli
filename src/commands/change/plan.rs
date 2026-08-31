use clap::Args as ClapArgs;

use crate::ci::{self, CiPlatform, AUDIENCE};
use crate::commands::pipeline::{
    authenticate, map_cli_error, require_proxy_mode, AuthenticatedSession,
};
use crate::errors::CliError;
use crate::observer_client::{ChangePlan, Client, OrchestratedPlan};
use crate::output::{emit_change_plan, emit_change_plan_with_diff, emit_change_plans};
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

    /// Air-gapped flow: package this directory into a digest-pinned bundle,
    /// upload it, and compile from that instead of letting Observer clone.
    /// Requires OBSERVER_API_URL and DESLICER_API_TOKEN (direct mode).
    ///
    /// Prefer the default git-sourced compile: a bundle carries git-lfs pointer
    /// files rather than their contents, so LFS-tracked config is not resolved.
    /// Use this only when Observer has no network path back to the repository.
    #[arg(long, requires = "target_group")]
    pub source_dir: Option<std::path::PathBuf>,

    /// Host group UUID. Required with --source-dir, and with
    /// OBSERVER_API_URL + DESLICER_API_TOKEN for git-sourced plans.
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

/// Bundle upload talks to Observer either with a static tools-scope key
/// (air-gap / laptop) or through the DAI CI proxy after OIDC resolve.
async fn bundle_flow_client(ctx: &Ctx, args: &Args) -> Result<Client, CliError> {
    if let (Some(base), Some(token)) = (
        ctx.observer_api_url.clone(),
        crate::observer_token::api_token(),
    ) {
        return Ok(Client::new(base, TokenSource::static_token(token)));
    }

    match authenticate(ctx, args.environment.as_deref(), None).await {
        Ok((_session, client)) => Ok(client),
        Err(err) => {
            if ctx.observer_api_url.is_some() {
                return Err(CliError::Other(
                    "--source-dir with --observer-api-url requires the \
                     DESLICER_API_TOKEN env var (an Observer API key with the \
                     `tools` scope)"
                        .into(),
                ));
            }
            Err(err)
        }
    }
}

async fn run_bundle_flow(ctx: &Ctx, args: &Args) -> Result<ChangePlan, CliError> {
    let source_dir = args.source_dir.as_deref().expect("clap guarantees value");
    let target_group = args.target_group.as_deref().expect("clap requires flag");

    let client = bundle_flow_client(ctx, args).await?;

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

    if session.is_device_session() {
        return map_cli_error(CliError::Other(
            "`change plan` without --source-dir starts a git-sourced compile. \
             Device sessions have no repository credentials. Re-run with \
             --source-dir <path> --target-group <id>."
                .into(),
        ));
    }

    // Direct mode: talking to Observer with a tools-scope key rather than through
    // the DAI CI proxy, so there is no OIDC resolve to supply repository identity
    // or environment discovery. Handle it before the proxy-mode gate.
    if session.is_observer_api_token() {
        return match run_direct_git_plan(&session, &client, &args).await {
            Ok(plan) => emit_change_plan(&plan),
            Err(err) => map_cli_error(err),
        };
    }

    if let Err(err) = require_proxy_mode(&session, "change plan") {
        return map_cli_error(err);
    }

    match run_git_plans(&ctx, &session, &client, &args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

fn environments_for_plan(explicit: Option<&str>, discovered: Vec<String>) -> Vec<Option<String>> {
    if let Some(env) = explicit {
        return vec![Some(env.to_string())];
    }
    if discovered.is_empty() {
        vec![None]
    } else {
        discovered.into_iter().map(Some).collect()
    }
}

async fn discover_environments(
    ctx: &Ctx,
    session: &AuthenticatedSession,
    explicit: Option<&str>,
) -> Result<Vec<Option<String>>, CliError> {
    if explicit.is_some() || ctx.observer_api_url.is_some() || session.platform == CiPlatform::Local
    {
        return Ok(environments_for_plan(explicit, Vec::new()));
    }
    let jwt = ci::provider_for(session.platform)
        .fetch_token(AUDIENCE)
        .await
        .map_err(CliError::from)?;
    let discovered = crate::resolver::resolve_environments(ctx, &jwt, session.platform).await?;
    Ok(environments_for_plan(explicit, discovered))
}

async fn compile_one_environment(
    client: &Client,
    environment: Option<&str>,
    no_wait: bool,
) -> Result<(ChangePlan, bool), CliError> {
    let created = client.create_plan_orchestrated(environment).await?;
    if no_wait || created.plan_row_id.is_none() {
        return Ok((orchestrated_as_change_plan(&created), false));
    }
    let plan = wait_for_compile(client, &created.plan_id)
        .await
        .map_err(CliError::Other)?;
    if is_compile_failure(&plan.status) {
        return Err(CliError::Other(format!(
            "plan compile failed with status: {}",
            plan.status
        )));
    }
    Ok((plan, true))
}

/// Git-sourced compile against Observer directly, without the DAI CI proxy.
///
/// The proxy path derives the repository, commit, and host group from the OIDC
/// claims; a tools-scope key carries none of that, so the repository identity
/// comes from the runner's own env and the host group from `--target-group`.
async fn run_direct_git_plan(
    session: &AuthenticatedSession,
    client: &Client,
    args: &Args,
) -> Result<ChangePlan, CliError> {
    let target_group = args.target_group.as_deref().ok_or_else(|| {
        CliError::Other(
            "git-sourced `change plan` with DESLICER_API_TOKEN requires \
             --target-group <host-group-uuid>. Run `deslicer groups list` \
             to find it."
                .into(),
        )
    })?;
    let identity = crate::ci::git_identity(session.platform)?;
    // Forwarded so Observer's ephemeral runner can clone a private repo it has no
    // GitHub App installation for. Absent is fine: Observer falls back to its own
    // credential and fails closed if it has none.
    let clone_token = crate::clone_token::from_env(session.platform);
    let plan = client
        .create_plan_from_git(
            &identity.repository_url,
            &identity.commit_sha,
            target_group,
            args.name.as_deref(),
        )
        .await?;
    client
        .trigger_compile_with_clone_token(&plan.id, &identity.commit_sha, clone_token.as_ref())
        .await?;
    if args.no_wait {
        return Ok(plan);
    }
    let compiled = wait_for_compile(client, plan.external_id())
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

async fn run_git_plans(
    ctx: &Ctx,
    session: &AuthenticatedSession,
    client: &Client,
    args: &Args,
) -> Result<i32, CliError> {
    let environments = discover_environments(ctx, session, args.environment.as_deref()).await?;
    let mut plans = Vec::new();
    let mut any_failed = false;
    let mut last_ready: Option<ChangePlan> = None;

    for environment in &environments {
        match compile_one_environment(client, environment.as_deref(), args.no_wait).await {
            Ok((plan, ready)) => {
                if ready {
                    last_ready = Some(plan.clone());
                }
                plans.push(plan);
            }
            Err(err) => {
                eprintln!("{err}");
                any_failed = true;
            }
        }
    }

    if plans.is_empty() {
        return Err(CliError::Other(
            "no plans were created for the resolved environments".into(),
        ));
    }

    let emit_code = if plans.len() == 1 {
        if let Some(ready) = last_ready.as_ref() {
            let diff = client
                .get_dry_run_diff(&ready.id)
                .await
                .ok()
                .and_then(|body| crate::diff_summary::diff_counts_from_observer_value(&body));
            emit_change_plan_with_diff(ready, diff.as_ref())
        } else {
            emit_change_plan(&plans[0])
        }
    } else {
        emit_change_plans(&plans)
    };

    if any_failed {
        Ok(1)
    } else {
        Ok(emit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::environments_for_plan;

    #[test]
    fn explicit_environment_wins() {
        let resolved = environments_for_plan(Some("prod"), vec!["staging".into(), "prod".into()]);
        assert_eq!(resolved, vec![Some("prod".to_string())]);
    }

    #[test]
    fn empty_discovery_falls_back_to_unscoped_plan() {
        let resolved = environments_for_plan(None, Vec::new());
        assert_eq!(resolved, vec![None]);
    }

    #[test]
    fn discovered_environments_fan_out() {
        let resolved = environments_for_plan(None, vec!["staging".into(), "prod".into()]);
        assert_eq!(
            resolved,
            vec![Some("staging".to_string()), Some("prod".to_string())]
        );
    }
}
