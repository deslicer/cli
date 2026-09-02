use clap::{Parser, Subcommand, ValueEnum};

use crate::ci::CiPlatform;

#[derive(Parser)]
#[command(
    name = "deslicer",
    version,
    long_version = concat!(
        env!("CARGO_PKG_VERSION"),
        " (",
        env!("DESLICER_GIT_SHA"),
        ")"
    ),
    about
)]
pub struct Cli {
    #[arg(
        long,
        env = "DESLICER_API_URL",
        default_value = "https://api.deslicer.ai",
        global = true
    )]
    pub deslicer_api_url: url::Url,

    #[arg(long, env = "OBSERVER_API_URL", global = true)]
    pub observer_api_url: Option<url::Url>,

    #[arg(long, value_enum, default_value_t = CiPlatformArg::Auto, global = true)]
    pub ci_platform: CiPlatformArg,

    #[arg(long, value_enum, default_value_t = LogFormat::Human, global = true)]
    pub log_format: LogFormat,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Talk to an agent (REPL by default; device session)
    #[command(subcommand)]
    Agent(crate::commands::agent::AgentCmd),
    #[command(subcommand)]
    Auth(crate::commands::auth::AuthCmd),
    #[command(subcommand)]
    Change(crate::commands::change::ChangeCmd),
    /// List host groups (`id` is the value for `change plan --target-group`)
    #[command(subcommand)]
    Groups(crate::commands::groups::GroupsCmd),
    /// List Ansible inventory groups, sync, or validate tenant environment YAML
    #[command(subcommand)]
    Inventory(crate::commands::inventory::InventoryCmd),
    /// Write CI templates for a config repo (optional --bind)
    Init(crate::commands::init::Args),
    /// Print documentation topic URLs (`--open` for a browser)
    Docs(crate::commands::docs::Args),
    /// Mint, list, or revoke enrollment tokens (device session)
    #[command(subcommand)]
    Enroll(crate::commands::enroll::EnrollCmd),
    /// Print worker install recipes (device session)
    #[command(subcommand)]
    Worker(crate::commands::worker::WorkerCmd),
    /// Provision or refresh a GitHub App config repo (device session)
    #[command(subcommand)]
    Repo(crate::commands::repo::RepoCmd),
    /// Generate shell completions (bash, zsh, fish)
    Completion(crate::commands::completion::Args),
    /// Update the deslicer binary to the latest release
    Update(crate::commands::update::Args),
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum CiPlatformArg {
    Auto,
    Github,
    Gitlab,
    Azure,
    Bitbucket,
    Local,
}

impl CiPlatformArg {
    pub fn as_override(self) -> Option<CiPlatform> {
        match self {
            CiPlatformArg::Auto => None,
            CiPlatformArg::Github => Some(CiPlatform::Github),
            CiPlatformArg::Gitlab => Some(CiPlatform::Gitlab),
            CiPlatformArg::Azure => Some(CiPlatform::Azure),
            CiPlatformArg::Bitbucket => Some(CiPlatform::Bitbucket),
            CiPlatformArg::Local => Some(CiPlatform::Local),
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum LogFormat {
    Human,
    Json,
}

#[derive(Debug, Clone)]
pub struct Ctx {
    pub deslicer_api_url: url::Url,
    pub observer_api_url: Option<url::Url>,
    pub ci_override: Option<CiPlatform>,
    pub log_format: LogFormat,
}

impl Cli {
    pub async fn run(self) -> i32 {
        let ctx = Ctx {
            deslicer_api_url: self.deslicer_api_url,
            observer_api_url: self.observer_api_url,
            ci_override: self.ci_platform.as_override(),
            log_format: self.log_format,
        };
        match self.command {
            Command::Agent(cmd) => crate::commands::agent::dispatch(ctx, cmd).await,
            Command::Auth(cmd) => crate::commands::auth::dispatch(ctx, cmd).await,
            Command::Change(cmd) => crate::commands::change::dispatch(ctx, cmd).await,
            Command::Groups(cmd) => crate::commands::groups::dispatch(ctx, cmd).await,
            Command::Inventory(cmd) => crate::commands::inventory::dispatch(ctx, cmd).await,
            Command::Init(args) => crate::commands::init::run(ctx, args).await,
            Command::Docs(args) => crate::commands::docs::run(ctx, args),
            Command::Enroll(cmd) => crate::commands::enroll::dispatch(ctx, cmd).await,
            Command::Worker(cmd) => crate::commands::worker::dispatch(ctx, cmd).await,
            Command::Repo(cmd) => crate::commands::repo::dispatch(ctx, cmd).await,
            Command::Completion(args) => crate::commands::completion::run(args),
            Command::Update(args) => crate::commands::update::run(args).await,
        }
    }
}
