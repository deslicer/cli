use clap::Subcommand;

use crate::Ctx;

pub mod mappings;
mod plan_matrix;

#[derive(Subcommand)]
pub enum EnvironmentsCmd {
    /// Build a CI matrix from environment destinations that contain apps
    PlanMatrix(plan_matrix::Args),
    /// Environment app mappings (Observer source of truth)
    #[command(subcommand)]
    Mappings(mappings::MappingsCmd),
}

pub async fn dispatch(ctx: Ctx, command: EnvironmentsCmd) -> i32 {
    match command {
        EnvironmentsCmd::PlanMatrix(args) => plan_matrix::run(ctx, args),
        EnvironmentsCmd::Mappings(cmd) => mappings::dispatch(ctx, cmd).await,
    }
}
