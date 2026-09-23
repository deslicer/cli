use clap::Subcommand;

use crate::Ctx;

mod plan_matrix;

#[derive(Subcommand)]
pub enum EnvironmentsCmd {
    /// Build a CI matrix from environment destinations that contain apps
    PlanMatrix(plan_matrix::Args),
}

pub async fn dispatch(ctx: Ctx, command: EnvironmentsCmd) -> i32 {
    match command {
        EnvironmentsCmd::PlanMatrix(args) => plan_matrix::run(ctx, args),
    }
}
