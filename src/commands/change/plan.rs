use clap::Args as ClapArgs;

use crate::ci::{self, CiPlatform, AUDIENCE};
use crate::cli::LogFormat;
use crate::commands::change::acs_inputs::resolve_acs_plan_inputs;
use crate::commands::change::plan_env::resolve_plan_environment;
use crate::commands::change::plan_name::resolve_plan_name;
use crate::commands::pipeline::{
    authenticate, map_cli_error, require_proxy_mode, AuthenticatedSession,
};
use crate::errors::CliError;
use crate::observer_client::{ChangePlan, Client, OrchestratedPlan};
use crate::output::{
    emit_change_plan, emit_change_plan_ci_outputs, emit_change_plan_with_diff, emit_change_plans,
};
use crate::pr_preview::{labels_for_plan_context, resolve_changed_paths, PrPreviewLabels};
use crate::token_source::TokenSource;
use crate::Ctx;

/// Honour subcommand `--json` the same way upgrade does (issue #210).
fn error_format(ctx: &Ctx, args: &Args) -> LogFormat {
    if args.json {
        LogFormat::Json
    } else {
        ctx.log_format
    }
}

fn plan_input_fingerprint(args: &Args) -> String {
    let source_dir = args
        .source_dir
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    crate::idempotency::fingerprint([
        "change plan",
        args.environment.as_deref().unwrap_or(""),
        args.destination.as_deref().unwrap_or(""),
        args.from_registry.as_deref().unwrap_or(""),
        source_dir.as_str(),
        args.target_group.as_deref().unwrap_or(""),
        args.name.as_deref().unwrap_or(""),
    ])
}

fn with_idempotency(client: Client, args: &Args) -> Client {
    client.with_idempotency_key(crate::idempotency::resolve(
        args.idempotency_key.as_deref(),
        &plan_input_fingerprint(args),
    ))
}

#[derive(ClapArgs)]
#[command(after_long_help = super::CHANGE_WORKFLOW_EXAMPLES)]
pub struct Args {
    /// Environment name or tenant slug for CI binding and plan creation.
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

    /// Host group UUID or exact group `name` (resolved via `GET /api/v1/groups`).
    /// Required with --source-dir. For OBSERVER_API_URL + DESLICER_API_TOKEN
    /// git plans, omit when the environment YAML has exactly one destination
    /// with apps — the CLI picks that inventory_group. Prefer the name from
    /// `.deslicer/environments/<env>.yml` over a static `TARGET_GROUP_ID`.
    #[arg(long)]
    pub target_group: Option<String>,

    /// Optional plan name shown in Deslicer Changes/Runs.
    /// When omitted in GitHub Actions, defaults to the pull request title
    /// (or the push commit subject). Local runs fall back to `git log -1`.
    #[arg(long)]
    pub name: Option<String>,

    /// Optional file of changed repo paths (one per line) for PR preview labeling.
    /// Clarity only — does not filter which apps the plan packs. When omitted,
    /// the CLI uses `DESLICER_CHANGED_PATHS` or a GitHub pull_request git range.
    #[arg(long, value_name = "FILE")]
    pub changed_paths_file: Option<std::path::PathBuf>,

    /// Comma/newline-separated changed paths (same labeling semantics as
    /// `--changed-paths-file`).
    #[arg(long, value_name = "PATHS")]
    pub changed_paths: Option<String>,

    /// Destination for non-git plans. ACS: `acs-stack:<stack-name>` creates a
    /// Pending Cloud ACS plan (never auto-approves). The environment comes from
    /// `--environment`, else the active session's tenant slug, else the only
    /// `.deslicer/environments/*.yml`; the CLI prints which one it used. Apps
    /// come from that file's `inventory_group: <stack>` (or `cloud`, or the
    /// only group with apps) and the `source_path` directories it lists.
    /// Exits 2 when the environment or apps cannot be resolved; an empty apps
    /// list is never sent. Mutually exclusive with `--source-dir` /
    /// `--from-registry`.
    #[arg(
        long,
        value_name = "SPEC",
        conflicts_with_all = ["source_dir", "from_registry"]
    )]
    pub destination: Option<String>,

    /// Plan a registry app release (`<app>@<ver>`, sent as
    /// `splunkbase:<app>/<ver>`). Works in any change-management mode.
    /// Needs a target group: `--target-group`, else the single env-YAML
    /// destination; exits 2 when neither resolves. The group must already pin
    /// this release. Creates a draft (`source_type=registry`), triggers the
    /// compile-runner and waits for Pending unless `--no-wait`. Never
    /// approves. Mutually exclusive with `--source-dir` / `--destination`.
    #[arg(
        long,
        value_name = "APP@VER",
        conflicts_with_all = ["source_dir", "destination"]
    )]
    pub from_registry: Option<String>,

    /// Print one JSON object with `plan_id` (and ACS item counts) on stdout.
    #[arg(long)]
    pub json: bool,

    /// Idempotency key for safe retries (`Idempotency-Key` header). Defaults in
    /// CI to run id + attempt/job + a hash of plan inputs; never defaults on a
    /// laptop from inputs alone.
    #[arg(long = "idempotency-key", env = "DESLICER_IDEMPOTENCY_KEY")]
    pub idempotency_key: Option<String>,
}

use super::lifecycle::{is_compile_failure, is_still_compiling};

/// Compile polling: the ephemeral compile-runner takes seconds to a few
/// minutes to clone, parse, diff, and post the plan draft.
const COMPILE_POLL_ATTEMPTS: u32 = 60;
const COMPILE_POLL_INTERVAL_SECS: u64 = 5;

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
        return Ok(with_idempotency(
            Client::new(base, TokenSource::static_token(token)).with_api_token_auth(),
            args,
        ));
    }

    match authenticate(ctx, args.environment.as_deref(), None).await {
        Ok((_session, client)) => Ok(with_idempotency(client, args)),
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

async fn resolve_plan_target_group(client: &Client, spec: &str) -> Result<String, CliError> {
    if crate::target_group::looks_like_uuid(spec) {
        return crate::target_group::resolve_target_group_id(spec, &[]);
    }
    let groups = client.list_groups().await?;
    let id = crate::target_group::resolve_target_group_id(spec, &groups)?;
    eprintln!("resolved --target-group {spec:?} → {id}");
    Ok(id)
}

/// Pick UUID/name from the flag, else a single env-YAML destination with apps.
async fn resolve_direct_plan_target_group(
    client: &Client,
    args: &Args,
    environment: Option<&str>,
) -> Result<String, CliError> {
    let env_yaml = match environment {
        Some(stem) => crate::target_group::read_environment_yaml(stem)?,
        None => None,
    };
    let spec = crate::target_group::choose_target_group_spec(
        args.target_group.as_deref(),
        env_yaml.as_deref(),
    )?;
    resolve_plan_target_group(client, &spec).await
}

async fn run_bundle_flow(ctx: &Ctx, args: &Args) -> Result<ChangePlan, CliError> {
    let source_dir = args.source_dir.as_deref().expect("clap guarantees value");
    let target_group_spec = args.target_group.as_deref().expect("clap requires flag");

    let client = bundle_flow_client(ctx, args).await?;
    client.ensure_bundle_plan_allowed().await?;
    let target_group = resolve_plan_target_group(&client, target_group_spec).await?;

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

    let plan_name = resolve_plan_name(args.name.as_deref());
    let plan = client
        .create_plan_from_bundle(&uploaded.id, &target_group, plan_name.as_deref())
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

fn parse_acs_stack_destination(spec: &str) -> Result<String, CliError> {
    const PREFIX: &str = "acs-stack:";
    let Some(rest) = spec.strip_prefix(PREFIX) else {
        return Err(CliError::InvalidInput(format!(
            "unsupported --destination {spec:?}; use acs-stack:<stack-name>"
        )));
    };
    let stack = rest.trim();
    if stack.is_empty() {
        return Err(CliError::InvalidInput(
            "--destination acs-stack: requires a non-empty stack name".into(),
        ));
    }
    Ok(stack.to_string())
}

async fn run_acs_destination_plan(ctx: &Ctx, args: &Args) -> Result<ChangePlan, CliError> {
    let spec = args
        .destination
        .as_deref()
        .expect("caller checks destination");
    let stack = parse_acs_stack_destination(spec)?;
    let environment = resolve_plan_environment(ctx, args.environment.as_deref())?;
    let origin = if args.environment.is_some() {
        "--environment"
    } else {
        "session tenant slug / single environment file"
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let roots = crate::environment_paths::search_roots_for(&cwd);
    let inputs = resolve_acs_plan_inputs(&stack, environment.as_deref(), &roots)?;
    eprintln!(
        "environment {} resolved from {origin} ({}); inventory_group {}; {} app(s): {}",
        inputs.request.environment_name,
        inputs.environment_file.display(),
        inputs.inventory_group,
        inputs.request.apps.len(),
        inputs
            .request
            .apps
            .iter()
            .map(|a| a.app_name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let (_session, client) = authenticate(ctx, environment.as_deref(), None).await?;
    let client = with_idempotency(client, args);
    let outcome = client
        .create_acs_project_plan(&stack, args.name.as_deref(), &inputs.request)
        .await?;
    let plan = outcome.as_change_plan();
    eprintln!(
        "ACS plan {} for stack {stack}: {} item(s), status {} (approve before deploy)",
        plan.external_id(),
        outcome.projected.item_count,
        plan.status
    );
    if args.json {
        let out = serde_json::json!({
            "plan_id": plan.external_id(),
            "id": plan.id,
            "status": plan.status,
            "stack": stack,
            "environment_name": inputs.request.environment_name,
            "environment_file": inputs.environment_file.display().to_string(),
            "inventory_group": inputs.inventory_group,
            "apps": inputs.request.apps.iter().map(|a| &a.app_name).collect::<Vec<_>>(),
            "item_count": outcome.projected.item_count,
            "items": outcome.projected.items,
        });
        let _ = crate::json_doc::print_success_compact(&out);
    }
    Ok(plan)
}

async fn run_registry_plan(ctx: &Ctx, args: &Args) -> Result<ChangePlan, CliError> {
    let spec = args
        .from_registry
        .as_deref()
        .expect("caller checks from_registry");
    let app_ref = crate::observer_client::parse_registry_app_ref(spec)?;
    let environment = resolve_plan_environment(ctx, args.environment.as_deref())?;
    let (_session, client) = authenticate(ctx, environment.as_deref(), None).await?;
    let client = with_idempotency(client, args);
    // Observer rejects registry plans without a group (registry_target_group_required);
    // fail locally with exit 2 instead of sending a request that cannot succeed.
    let env_yaml = match environment.as_deref() {
        Some(stem) => crate::target_group::read_environment_yaml(stem)?,
        None => None,
    };
    let group_spec = crate::target_group::choose_target_group_spec(
        args.target_group.as_deref(),
        env_yaml.as_deref(),
    )
    .map_err(|err| {
        CliError::InvalidInput(format!(
            "--from-registry needs a target group: {err}. Pass --target-group <name|uuid>."
        ))
    })?;
    let target_group = resolve_plan_target_group(&client, &group_spec).await?;
    let plan_name = resolve_plan_name(args.name.as_deref());
    let plan = client
        .create_plan_from_registry(&app_ref, &target_group, plan_name.as_deref())
        .await?;
    eprintln!(
        "registry plan created for {} on group {target_group} (draft)",
        crate::observer_client::registry_source_reference(&app_ref)
    );
    // Registry plans have no git ref; the source is the group's pinned blob.
    client.trigger_compile(&plan.id, "registry").await?;
    if args.no_wait {
        return Ok(plan);
    }
    let compiled = wait_for_compile(&client, plan.external_id())
        .await
        .map_err(CliError::Other)?;
    if is_compile_failure(&compiled.status) {
        return Err(CliError::Other(format!(
            "registry plan compile failed with status: {}",
            compiled.status
        )));
    }
    eprintln!("registry plan ready (Pending — approve before deploy)");
    Ok(compiled)
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let format = error_format(&ctx, &args);
    if args.source_dir.is_some() {
        return match run_bundle_flow(&ctx, &args).await {
            Ok(plan) => emit_change_plan(&plan),
            Err(err) => map_cli_error(format, err),
        };
    }

    if args.destination.is_some() {
        return match run_acs_destination_plan(&ctx, &args).await {
            Ok(plan) if args.json => emit_change_plan_ci_outputs(&plan),
            Ok(plan) => emit_change_plan(&plan),
            Err(err) => map_cli_error(format, err),
        };
    }

    if args.from_registry.is_some() {
        return match run_registry_plan(&ctx, &args).await {
            Ok(plan) => emit_change_plan(&plan),
            Err(err) => map_cli_error(format, err),
        };
    }

    let environment = match resolve_plan_environment(&ctx, args.environment.as_deref()) {
        Ok(value) => value,
        Err(err) => return map_cli_error(format, err),
    };
    let (session, client) = match authenticate(&ctx, environment.as_deref(), None).await {
        Ok((session, client)) => (session, with_idempotency(client, &args)),
        Err(err) => return map_cli_error(format, err),
    };

    if session.is_device_session() {
        return map_cli_error(
            format,
            CliError::Other(
                "`change plan` without --source-dir starts a git-sourced compile. \
             Device sessions have no repository credentials. Use CI with Path A OIDC, \
             or connect the GitHub App and bind the environment in the portal. \
             Bundle upload (`--source-dir`) is only for direct-mode tenants, not \
             git-backed change management."
                    .into(),
            ),
        );
    }

    // Direct mode: talking to Observer with a tools-scope key rather than through
    // the DAI CI proxy, so there is no OIDC resolve to supply repository identity
    // or environment discovery. Handle it before the proxy-mode gate.
    if session.is_observer_api_token() {
        return match run_direct_git_plan(&session, &client, &args, environment.as_deref()).await {
            Ok(plan) => emit_ready_plan(&client, &args, environment.as_deref(), plan).await,
            Err(err) => map_cli_error(format, err),
        };
    }

    if let Err(err) = require_proxy_mode(&session, "change plan") {
        return map_cli_error(format, err);
    }

    match run_git_plans(&ctx, &session, &client, &args, environment.as_deref()).await {
        Ok(code) => code,
        Err(err) => map_cli_error(format, err),
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
    target_group_id: Option<&str>,
    no_wait: bool,
) -> Result<(ChangePlan, bool), CliError> {
    let created = client
        .create_plan_orchestrated(environment, target_group_id)
        .await?;
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
/// comes from the runner's own env and the host group from `--target-group`
/// (or a single destination in the environment YAML).
async fn run_direct_git_plan(
    session: &AuthenticatedSession,
    client: &Client,
    args: &Args,
    environment: Option<&str>,
) -> Result<ChangePlan, CliError> {
    let target_group = resolve_direct_plan_target_group(client, args, environment).await?;
    let identity = crate::ci::git_identity(session.platform)?;
    // Forwarded so Observer's ephemeral runner can clone a private repo it has no
    // GitHub App installation for. Absent is fine: Observer falls back to its own
    // credential and fails closed if it has none.
    let clone_token = crate::clone_token::from_env(session.platform);
    let plan_name = resolve_plan_name(args.name.as_deref());
    let plan = client
        .create_plan_from_git(
            &identity.repository_url,
            &identity.commit_sha,
            &target_group,
            plan_name.as_deref(),
        )
        .await?;
    if !is_still_compiling(&plan.status) {
        return Ok(plan);
    }
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
    environment: Option<&str>,
) -> Result<i32, CliError> {
    let environments = discover_environments(ctx, session, environment).await?;
    let target_group_id = match args.target_group.as_deref() {
        Some(spec) => Some(resolve_plan_target_group(client, spec).await?),
        None => None,
    };
    let mut plans = Vec::new();
    let mut any_failed = false;
    let mut last_ready: Option<ChangePlan> = None;

    for environment in &environments {
        match compile_one_environment(
            client,
            environment.as_deref(),
            target_group_id.as_deref(),
            args.no_wait,
        )
        .await
        {
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
        if let Some(ready) = last_ready {
            emit_ready_plan(client, args, environment, ready).await
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

fn preview_labels_for(
    args: &Args,
    environment: Option<&str>,
    diff_body: Option<&serde_json::Value>,
) -> Option<PrPreviewLabels> {
    let changed = resolve_changed_paths(
        args.changed_paths_file.as_deref(),
        args.changed_paths.as_deref(),
    )?;
    let env_yaml = environment.and_then(|stem| {
        crate::target_group::read_environment_yaml(stem)
            .ok()
            .flatten()
    });
    labels_for_plan_context(diff_body, env_yaml.as_deref(), &changed)
}

async fn emit_ready_plan(
    client: &Client,
    args: &Args,
    environment: Option<&str>,
    plan: ChangePlan,
) -> i32 {
    let diff_body = client.get_dry_run_diff(&plan.id).await.ok();
    let counts = diff_body
        .as_ref()
        .and_then(crate::diff_summary::diff_counts_from_observer_value);
    let preview = preview_labels_for(args, environment, diff_body.as_ref());
    if let Some(ref labels) = preview {
        eprintln!("{}", labels.human_summary());
    }
    emit_change_plan_with_diff(&plan, counts.as_ref(), preview.as_ref())
}

#[cfg(test)]
mod tests {
    use super::{environments_for_plan, parse_acs_stack_destination};

    #[test]
    fn acs_destination_parses_stack_name() {
        assert_eq!(
            parse_acs_stack_destination("acs-stack:prod").unwrap(),
            "prod"
        );
        assert!(parse_acs_stack_destination("acs-stack:").is_err());
        assert!(parse_acs_stack_destination("k8s:ns").is_err());
    }

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

    #[test]
    fn compile_is_only_triggered_while_still_compiling() {
        use crate::commands::change::lifecycle;
        assert!(lifecycle::is_still_compiling("draft"));
        assert!(lifecycle::is_still_compiling("compiling"));
        assert!(lifecycle::is_still_compiling("compile_pending"));
        assert!(!lifecycle::is_still_compiling("pending_approval"));
        assert!(!lifecycle::is_still_compiling("approved_unsigned"));
        assert!(!lifecycle::is_still_compiling("executing"));
        assert!(!lifecycle::is_still_compiling("completed"));
        assert!(!lifecycle::is_still_compiling("failed"));
    }
}
