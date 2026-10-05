use clap::Subcommand;

use crate::Ctx;

pub mod approve;
pub mod deploy;
pub mod lifecycle;
pub mod plan;
mod plan_env;
mod plan_name;
pub mod reject;
pub mod show;
pub mod status;
pub mod validate;
pub mod verify;

pub const CHANGE_WORKFLOW_EXAMPLES: &str = concat!(
    "Examples:\n",
    "  deslicer change plan --environment production\n", // pragma: allowlist secret
    "  deslicer change show --plan-id <plan-id>\n", // pragma: allowlist secret
    "  deslicer change validate --plan-id <plan-id> --environment production\n", // pragma: allowlist secret
    "  deslicer change approve --plan-id <plan-id> --environment production\n", // pragma: allowlist secret
    "  deslicer change deploy --plan-id <plan-id> --environment production\n", // pragma: allowlist secret
    "  deslicer change status --plan-id <plan-id>\n", // pragma: allowlist secret
);

#[derive(Subcommand)]
pub enum ChangeCmd {
    /// Create a change plan from the current repo commit (or a local bundle).
    Plan(plan::Args),
    /// Show one plan or list recent plans for an environment.
    Show(show::Args),
    /// Approve a plan pending human review (CI proxy attests the approver).
    Approve(approve::Args),
    /// Reject a plan with an optional reason.
    Reject(reject::Args),
    /// Queue and monitor rollout of an approved plan.
    Deploy(deploy::Args),
    /// Run or inspect validation for an existing persisted plan.
    Validate(validate::Args),
    /// Re-run compile verification and refresh the dry-run diff for a plan.
    Verify(verify::Args),
    /// Report plan lifecycle status and rollout progress (for CI wait steps).
    Status(status::Args),
}

pub async fn dispatch(ctx: Ctx, cmd: ChangeCmd) -> i32 {
    match cmd {
        ChangeCmd::Plan(args) => plan::run(ctx, args).await,
        ChangeCmd::Show(args) => show::run(ctx, args).await,
        ChangeCmd::Approve(args) => approve::run(ctx, args).await,
        ChangeCmd::Reject(args) => reject::run(ctx, args).await,
        ChangeCmd::Deploy(args) => deploy::run(ctx, args).await,
        ChangeCmd::Validate(args) => validate::run(ctx, args).await,
        ChangeCmd::Verify(args) => verify::run(ctx, args).await,
        ChangeCmd::Status(args) => status::run(ctx, args).await,
    }
}
