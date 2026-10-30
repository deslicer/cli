use std::io::IsTerminal;

use clap::{Parser, Subcommand, ValueEnum};

use crate::ci::CiPlatform;
use crate::reporting::{emit_log_format_deprecation, emit_output_conflict, resolve_output_format};

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
    about,
    after_help = "Exit codes: see docs/exit-codes.md in the deslicer/cli repository."
)]
pub struct Cli {
    /// API base URL used for resolve-backend and OIDC exchange.
    #[arg(
        long,
        env = "DESLICER_API_URL",
        default_value = "https://api.deslicer.ai",
        global = true
    )]
    pub deslicer_api_url: url::Url,

    /// Observer API endpoint for direct access. Requires DESLICER_API_TOKEN.
    #[arg(long, env = "OBSERVER_API_URL", global = true)]
    pub observer_api_url: Option<url::Url>,

    /// CI platform used for OIDC, output sinks, and git identity (default: auto-detect).
    #[arg(long, value_enum, default_value_t = CiPlatformArg::Auto, global = true)]
    pub ci_platform: CiPlatformArg,

    /// Output format (`json` or `human`). Default: json when stdout is not a TTY.
    #[arg(
        short = 'o',
        long = "output",
        value_enum,
        global = true,
        env = "DESLICER_OUTPUT"
    )]
    pub output: Option<LogFormat>,

    /// Emit JSON (alias for `--output json`).
    #[arg(long, global = true, hide = true)]
    pub json: bool,

    /// Deprecated: prefer `--output` / `-o`.
    #[arg(long, value_enum, global = true)]
    pub log_format: Option<LogFormat>,

    /// Per-request HTTP timeout in seconds (default 120). Env: `DESLICER_TIMEOUT`.
    #[arg(
        long = "timeout",
        global = true,
        env = "DESLICER_TIMEOUT",
        value_name = "SECS"
    )]
    pub timeout: Option<u64>,

    /// Retries for idempotent GETs on 429/502/503/504 and connect errors (default 2; 0 disables).
    #[arg(
        long = "retries",
        global = true,
        env = "DESLICER_RETRIES",
        value_name = "N"
    )]
    pub retries: Option<u64>,

    /// Never prompt or open a browser. Implied by `CI=true` or a non-TTY stdin.
    #[arg(
        long = "no-input",
        alias = "non-interactive",
        global = true,
        env = "DESLICER_NO_INPUT",
        action = clap::ArgAction::SetTrue
    )]
    pub no_input: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Talk to an agent (REPL by default; device session)
    #[command(subcommand)]
    Agent(crate::commands::agent::AgentCmd),
    /// Sign in, check CI identity, and manage stored credentials
    #[command(subcommand)]
    Auth(crate::commands::auth::AuthCmd),
    /// Create, inspect, approve, and deploy configuration change plans
    #[command(subcommand)]
    Change(crate::commands::change::ChangeCmd),
    /// Splunk Cloud ACS stacks, credentials, and observe (read-only)
    #[command(subcommand)]
    Acs(crate::commands::acs::AcsCmd),
    /// DAI routines (list, enable/disable, run, runs, report)
    #[command(subcommand)]
    Routine(crate::commands::routine::RoutineCmd),
    /// Skills marketplace (list, import)
    #[command(subcommand)]
    Skill(crate::commands::skill::SkillCmd),
    /// Catalog blueprints (list, show, multi-target plan)
    #[command(subcommand)]
    Catalog(crate::commands::catalog::CatalogCmd),
    /// Remediate analyse / draft (draft plan id)
    #[command(subcommand)]
    Remediate(crate::commands::remediate::RemediateCmd),
    /// Tenant license status and bind
    #[command(subcommand)]
    License(crate::commands::license::LicenseCmd),
    /// Model catalogue and BYOK
    #[command(subcommand)]
    Models(crate::commands::models::ModelsCmd),
    /// Billing usage and credits (read-only)
    #[command(subcommand)]
    Billing(crate::commands::billing::BillingCmd),
    /// Org roles, teams, switch, and plan settings
    #[command(subcommand)]
    Org(crate::commands::org::OrgCmd),
    /// List host groups (`id` or exact `name` for `change plan --target-group`)
    #[command(subcommand)]
    Groups(crate::commands::groups::GroupsCmd),
    /// List Ansible inventory groups, sync, or validate tenant environment YAML
    #[command(subcommand)]
    Inventory(crate::commands::inventory::InventoryCmd),
    /// Build CI helpers from `.deslicer/environments` files
    #[command(subcommand)]
    Environments(crate::commands::environments::EnvironmentsCmd),
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
    /// Splunk package upgrade intent (plan / flapping-gate status)
    #[command(subcommand)]
    Upgrade(crate::commands::upgrade::UpgradeCmd),
    /// Emit machine-readable command / flag / exit-code schema (JSON)
    Schema(crate::commands::schema::Args),
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
        let stdout_is_tty = std::io::stdout().is_terminal();
        let log_format =
            match resolve_output_format(self.output, self.json, self.log_format, stdout_is_tty) {
                Ok(format) => format,
                Err(_) => {
                    let fallback = if !stdout_is_tty
                        || self.json
                        || self.output == Some(LogFormat::Json)
                        || self.log_format == Some(LogFormat::Json)
                    {
                        LogFormat::Json
                    } else {
                        LogFormat::Human
                    };
                    return emit_output_conflict(fallback);
                }
            };
        if self.log_format.is_some() {
            emit_log_format_deprecation();
        }
        crate::runtime_limits::set_from_cli(self.timeout, self.retries);
        crate::interactive::set_no_input(self.no_input);
        let ctx = Ctx {
            deslicer_api_url: self.deslicer_api_url,
            observer_api_url: self.observer_api_url,
            ci_override: self.ci_platform.as_override(),
            log_format,
        };
        if let Err(err) = crate::observer_token::ObserverTokenConfiguration::validate(&ctx) {
            return crate::reporting::emit_cli_error(ctx.log_format, &err);
        }
        match self.command {
            Command::Agent(cmd) => crate::commands::agent::dispatch(ctx, cmd).await,
            Command::Auth(cmd) => crate::commands::auth::dispatch(ctx, cmd).await,
            Command::Change(cmd) => crate::commands::change::dispatch(ctx, cmd).await,
            Command::Acs(cmd) => crate::commands::acs::dispatch(ctx, cmd).await,
            Command::Routine(cmd) => crate::commands::routine::dispatch(ctx, cmd).await,
            Command::Skill(cmd) => crate::commands::skill::dispatch(ctx, cmd).await,
            Command::Catalog(cmd) => crate::commands::catalog::dispatch(ctx, cmd).await,
            Command::Remediate(cmd) => crate::commands::remediate::dispatch(ctx, cmd).await,
            Command::License(cmd) => crate::commands::license::dispatch(ctx, cmd).await,
            Command::Models(cmd) => crate::commands::models::dispatch(ctx, cmd).await,
            Command::Billing(cmd) => crate::commands::billing::dispatch(ctx, cmd).await,
            Command::Org(cmd) => crate::commands::org::dispatch(ctx, cmd).await,
            Command::Groups(cmd) => crate::commands::groups::dispatch(ctx, cmd).await,
            Command::Inventory(cmd) => crate::commands::inventory::dispatch(ctx, cmd).await,
            Command::Environments(cmd) => crate::commands::environments::dispatch(ctx, cmd).await,
            Command::Init(args) => crate::commands::init::run(ctx, args).await,
            Command::Docs(args) => crate::commands::docs::run(ctx, args),
            Command::Enroll(cmd) => crate::commands::enroll::dispatch(ctx, cmd).await,
            Command::Worker(cmd) => crate::commands::worker::dispatch(ctx, cmd).await,
            Command::Repo(cmd) => crate::commands::repo::dispatch(ctx, cmd).await,
            Command::Completion(args) => crate::commands::completion::run(args),
            Command::Update(args) => crate::commands::update::run(ctx, args).await,
            Command::Upgrade(cmd) => crate::commands::upgrade::dispatch(ctx, cmd).await,
            Command::Schema(args) => crate::commands::schema::run(ctx, args),
        }
    }
}
