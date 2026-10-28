//! Org roles, teams, switch, and plan settings (device session → deslicer-ai / Observer).

use clap::Subcommand;

use crate::Ctx;

mod client;
mod roles;
mod settings;
mod switch;
mod teams;

#[derive(Subcommand)]
pub enum OrgCmd {
    /// List roles for the current org/tenant
    Roles(roles::Args),
    /// List teams for the current org/tenant
    Teams(teams::Args),
    /// Switch the active tenant/org (dry-run unless --yes)
    Switch(switch::Args),
    /// Plan approval / execution settings
    #[command(subcommand)]
    Settings(settings::SettingsCmd),
}

pub async fn dispatch(ctx: Ctx, cmd: OrgCmd) -> i32 {
    match cmd {
        OrgCmd::Roles(args) => roles::run(ctx, args).await,
        OrgCmd::Teams(args) => teams::run(ctx, args).await,
        OrgCmd::Switch(args) => switch::run(ctx, args).await,
        OrgCmd::Settings(cmd) => settings::dispatch(ctx, cmd).await,
    }
}
