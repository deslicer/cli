use clap::{Args as ClapArgs, Subcommand};
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::{find_mapping, Client, EnvironmentAppMapping, MappingUpsert};
use crate::target_group::{looks_like_uuid, resolve_target_group_id};
use crate::Ctx;

#[derive(Subcommand)]
pub enum MappingsCmd {
    /// List environment app mappings (Observer source of truth)
    ///
    /// Examples:
    ///   deslicer environments mappings list --environment prod
    ///   deslicer --log-format json environments mappings list --environment prod
    List(ListArgs),
    /// Set / upsert one app mapping (dry-run unless --yes)
    ///
    /// Upserts only (replace=false); sibling mappings are never deleted.
    /// Observer accepts direct writes only when the environment mapping source
    /// is `db`.
    ///
    /// Examples:
    ///   deslicer environments mappings set --environment prod --app ta_nix \
    ///     --source-path apps/ta_nix --group indexers --yes
    Set(SetArgs),
    /// Adopt an app already on a host group into the mapping (dry-run unless --yes)
    ///
    /// Records the app for the group without a repo source path, so Observer
    /// tracks it as managed. Use `set` later to attach a source path.
    ///
    /// Examples:
    ///   deslicer environments mappings adopt --environment prod --app ta_nix --group indexers --yes
    Adopt(AdoptArgs),
}

#[derive(ClapArgs)]
pub struct ListArgs {
    /// Environment name (required; Observer SoT is per environment)
    #[arg(long)]
    pub environment: String,
}

#[derive(ClapArgs)]
pub struct SetArgs {
    /// Environment name
    #[arg(long)]
    pub environment: String,

    /// App name to map
    #[arg(long)]
    pub app: String,

    /// Repo-relative source path for the app
    #[arg(long)]
    pub source_path: String,

    /// Host group: UUID or exact name from `deslicer groups list`
    #[arg(long)]
    pub group: String,

    /// Optional destination directory on the host
    #[arg(long)]
    pub dest_dir: Option<String>,

    /// Optional single-host scope (host UUID)
    #[arg(long)]
    pub host: Option<String>,

    /// Apply the mapping. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

#[derive(ClapArgs)]
pub struct AdoptArgs {
    /// Environment name
    #[arg(long)]
    pub environment: String,

    /// App name discovered on the host group
    #[arg(long)]
    pub app: String,

    /// Host group: UUID or exact name from `deslicer groups list`
    #[arg(long)]
    pub group: String,

    /// Optional single-host scope (host UUID)
    #[arg(long)]
    pub host: Option<String>,

    /// Apply the adopt. Without this flag, print a dry-run and exit.
    #[arg(long)]
    pub yes: bool,
}

pub async fn dispatch(ctx: Ctx, cmd: MappingsCmd) -> i32 {
    match cmd {
        MappingsCmd::List(args) => list(ctx, args).await,
        MappingsCmd::Set(args) => set(ctx, args).await,
        MappingsCmd::Adopt(args) => adopt(ctx, args).await,
    }
}

async fn list(ctx: Ctx, args: ListArgs) -> i32 {
    let (_session, client) = match authenticate(&ctx, Some(&args.environment), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let mappings = match client.list_environment_mappings(&args.environment).await {
        Ok(mappings) => mappings,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(&mappings) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize mappings: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_mappings_human(&mappings));
            0
        }
    }
}

fn validate_host(host: Option<&str>) -> Result<(), CliError> {
    match host {
        Some(h) if !looks_like_uuid(h) => Err(CliError::InvalidInput(
            "--host must be a host UUID (see `deslicer hosts list`)".into(),
        )),
        _ => Ok(()),
    }
}

async fn resolve_group(client: &Client, spec: &str) -> Result<String, CliError> {
    if looks_like_uuid(spec) {
        return Ok(spec.trim().to_string());
    }
    let groups = client.list_groups().await?;
    resolve_target_group_id(spec, &groups)
}

async fn upsert_one(
    ctx: &Ctx,
    environment: &str,
    group: &str,
    build: impl FnOnce(String) -> MappingUpsert,
) -> i32 {
    let (_session, client) = match authenticate(ctx, Some(environment), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let group_id = match resolve_group(&client, group).await {
        Ok(id) => id,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let entry = build(group_id);
    match client
        .upsert_environment_mappings(environment, std::slice::from_ref(&entry))
        .await
    {
        Ok(rows) => match find_mapping(&rows, &entry) {
            Some(row) => emit_mapping(ctx, row),
            None => map_cli_error(
                ctx.log_format,
                CliError::Other(format!(
                    "Observer accepted the write but did not return mapping {} for group {}",
                    entry.app_name, entry.host_group_id
                )),
            ),
        },
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

async fn set(ctx: Ctx, args: SetArgs) -> i32 {
    if let Err(err) = validate_host(args.host.as_deref()) {
        return map_cli_error(ctx.log_format, err);
    }
    if !args.yes {
        print_set_dry_run(&ctx, &args);
        return 0;
    }
    upsert_one(&ctx, &args.environment, &args.group, |host_group_id| {
        MappingUpsert {
            host_group_id,
            app_name: args.app.clone(),
            source_path: Some(args.source_path.clone()),
            dest_dir: args.dest_dir.clone(),
            target_host_id: args.host.clone(),
        }
    })
    .await
}

async fn adopt(ctx: Ctx, args: AdoptArgs) -> i32 {
    if let Err(err) = validate_host(args.host.as_deref()) {
        return map_cli_error(ctx.log_format, err);
    }
    if !args.yes {
        print_adopt_dry_run(&ctx, &args);
        return 0;
    }
    upsert_one(&ctx, &args.environment, &args.group, |host_group_id| {
        MappingUpsert {
            host_group_id,
            app_name: args.app.clone(),
            source_path: None,
            dest_dir: None,
            target_host_id: args.host.clone(),
        }
    })
    .await
}

fn emit_mapping(ctx: &Ctx, mapping: &EnvironmentAppMapping) -> i32 {
    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(mapping) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize mapping: {err}");
                1
            }
        },
        LogFormat::Human => {
            println!(
                "mapping ok: app={} source_path={} group={} host={} state={}",
                mapping.app_name,
                mapping.source_path.as_deref().unwrap_or("-"),
                mapping.inventory_group.as_deref().unwrap_or("-"),
                mapping.target_host.as_deref().unwrap_or("-"),
                mapping.state.as_deref().unwrap_or("-")
            );
            0
        }
    }
}

fn print_set_dry_run(ctx: &Ctx, args: &SetArgs) {
    match ctx.log_format {
        LogFormat::Json => {
            let _ = crate::json_doc::print_success(&json!({
                "dry_run": true,
                "action": "set",
                "environment": args.environment,
                "app": args.app,
                "source_path": args.source_path,
                "group": args.group,
                "dest_dir": args.dest_dir,
                "host": args.host,
            }));
        }
        LogFormat::Human => print!("{}", set_dry_run_text(args)),
    }
}

fn print_adopt_dry_run(ctx: &Ctx, args: &AdoptArgs) {
    match ctx.log_format {
        LogFormat::Json => {
            let _ = crate::json_doc::print_success(&json!({
                "dry_run": true,
                "action": "adopt",
                "environment": args.environment,
                "app": args.app,
                "group": args.group,
                "host": args.host,
            }));
        }
        LogFormat::Human => print!("{}", adopt_dry_run_text(args)),
    }
}

fn set_dry_run_text(args: &SetArgs) -> String {
    format!(
        "dry-run: would set mapping app={} source_path={} group={} host={} in environment {}\n\
         pass --yes to apply\n",
        args.app,
        args.source_path,
        args.group,
        args.host.as_deref().unwrap_or("-"),
        args.environment
    )
}

fn adopt_dry_run_text(args: &AdoptArgs) -> String {
    format!(
        "dry-run: would adopt app={} group={} host={} into environment {}\n\
         pass --yes to apply\n",
        args.app,
        args.group,
        args.host.as_deref().unwrap_or("-"),
        args.environment
    )
}

fn format_mappings_human(mappings: &[EnvironmentAppMapping]) -> String {
    if mappings.is_empty() {
        return "No environment app mappings.\n".to_string();
    }
    let mut lines = vec!["APP  SOURCE_PATH  GROUP  HOST  STATE  SOURCE".to_string()];
    for m in mappings {
        lines.push(format!(
            "{}  {}  {}  {}  {}  {}",
            m.app_name,
            m.source_path.as_deref().unwrap_or("-"),
            m.inventory_group.as_deref().unwrap_or("-"),
            m.target_host.as_deref().unwrap_or("-"),
            m.state.as_deref().unwrap_or("-"),
            m.source.as_deref().unwrap_or("-")
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn set_dry_run_mentions_yes() {
        let text = set_dry_run_text(&SetArgs {
            environment: "prod".into(),
            app: "ta_nix".into(),
            source_path: "apps/ta_nix".into(),
            group: "indexers".into(),
            dest_dir: None,
            host: None,
            yes: false,
        });
        assert!(text.contains("--yes"));
        assert!(text.contains("ta_nix"));
    }

    #[test]
    fn host_must_be_uuid() {
        assert!(validate_host(Some("idx01")).is_err());
        assert!(validate_host(Some("0b8f2f0e-1111-4222-8333-444455556666")).is_ok());
        assert!(validate_host(None).is_ok());
    }

    #[test]
    fn help_requires_yes_for_set_and_adopt() {
        let mut cmd = crate::cli::Cli::command();
        let env = cmd
            .find_subcommand_mut("environments")
            .expect("environments");
        let mappings = env.find_subcommand_mut("mappings").expect("mappings");
        for name in ["set", "adopt"] {
            let sub = mappings.find_subcommand_mut(name).expect(name);
            let mut buf = Vec::new();
            sub.write_long_help(&mut buf).expect("help");
            let help = String::from_utf8(buf).expect("utf8");
            assert!(help.contains("--yes"), "{name} help missing --yes");
            assert!(help.contains("--group"), "{name} help missing --group");
        }
    }
}
