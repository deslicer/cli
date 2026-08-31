use clap::Args as ClapArgs;

use crate::commands::pipeline::{authenticate, map_cli_error, require_proxy_mode};
use crate::diff_summary::diff_counts_from_observer_value;
use crate::errors::CliError;
use crate::output::{emit_change_plan_with_diff, emit_message};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// External plan id (the `plan_id` output of `change plan`).
    #[arg(long)]
    pub plan_id: String,

    #[arg(long)]
    pub environment: Option<String>,

    /// Git ref for the dry-run compile; defaults to the commit the plan was
    /// created from.
    #[arg(long)]
    pub git_ref: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (session, client) =
        match authenticate(&ctx, args.environment.as_deref(), Some(&args.plan_id)).await {
            Ok(pair) => pair,
            Err(err) => return map_cli_error(err),
        };

    // Direct mode with a tools-scope key re-compiles against Observer itself, so
    // it does not need the DAI CI proxy that `verify_plan_orchestrated` goes through.
    if !session.backend.proxy_mode && !session.is_observer_api_token() {
        if let Err(err) = require_proxy_mode(&session, "change verify") {
            return map_cli_error(err);
        }
    }

    // Resolve the internal row id — the compile-runner and diff endpoints
    // are keyed by it, while --plan-id carries the external identifier.
    let plan = match client.get_plan(&args.plan_id).await {
        Ok(plan) => plan,
        Err(err) => return map_cli_error(err),
    };

    if session.is_device_session() {
        if args.git_ref.is_some() {
            return map_cli_error(CliError::Other(
                "`change verify --git-ref` requires CI OIDC. Device sessions \
                 can only re-compile bundle-sourced plans."
                    .into(),
            ));
        }
        if let Err(err) = client.trigger_compile(&plan.id, "bundle").await {
            eprintln!("verification failed: {err}");
            return map_cli_error(err);
        }
    } else if session.is_observer_api_token() {
        let git_ref = match args.git_ref.clone() {
            Some(value) => value,
            None => match crate::ci::git_identity(session.platform) {
                Ok(identity) => identity.commit_sha,
                Err(err) => return map_cli_error(err),
            },
        };
        // Same clone credential as `change plan`: re-compiling a git-sourced plan
        // needs a fresh clone, so it needs the same repository access.
        let clone_token = crate::clone_token::from_env(session.platform);
        if let Err(err) = client
            .trigger_compile_with_clone_token(&plan.id, &git_ref, clone_token.as_ref())
            .await
        {
            eprintln!("verification failed: {err}");
            return map_cli_error(err);
        }
    } else if let Err(err) = client
        .verify_plan_orchestrated(&plan.id, args.git_ref.as_deref())
        .await
    {
        eprintln!("verification failed: {err}");
        return map_cli_error(err);
    }

    // Refresh lifecycle status after verify (compile may have advanced the plan).
    let plan = match client.get_plan(&args.plan_id).await {
        Ok(plan) => plan,
        Err(_) => plan,
    };

    // The diff is best-effort output: verification already succeeded above.
    match client.get_dry_run_diff(&plan.id).await {
        Ok(diff) => {
            println!("{}", serde_json::to_string(&diff).unwrap_or_default());
            let counts = diff_counts_from_observer_value(&diff);
            if let Some(ref counts) = counts {
                emit_message(&crate::output::diff_count_pairs(counts));
            }
            emit_change_plan_with_diff(&plan, counts.as_ref())
        }
        Err(err) => {
            eprintln!("dry-run accepted, but the diff could not be fetched: {err}");
            emit_change_plan_with_diff(&plan, None)
        }
    }
}
