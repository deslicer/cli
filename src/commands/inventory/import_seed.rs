//! `deslicer inventory import-seed` — declare hosts from a lab Ansible inventory.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use serde_json::json;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::{AssignHostRequest, Client, CreateDeclaredHostRequest, DeclaredHost};
use crate::seed_inventory::{parse_seed_yaml, role_requires_cluster_number, SeedHost};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Ansible inventory YAML with `all.hosts` (hostname, ansible_host, intended_inventory_role).
    #[arg(long, value_name = "FILE")]
    pub file: PathBuf,

    #[arg(long)]
    pub environment: Option<String>,

    /// Also POST `/api/v1/inventory/assign` using `intended_inventory_role`.
    #[arg(long)]
    pub assign: bool,

    /// Availability group for `--assign` (default `ag1`).
    #[arg(long, default_value = "ag1")]
    pub availability_group: String,

    /// Cluster number for SHC / index-cluster roles (default `1` when needed).
    #[arg(long)]
    pub cluster_number: Option<i32>,

    /// Parse and report without calling Observer.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug)]
struct ImportRow {
    hostname: String,
    ansible_host: Option<String>,
    role: Option<String>,
    declared: Option<DeclaredHost>,
    assigned_group: Option<String>,
    error: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(&ctx, args).await {
        Ok(code) => code,
        Err(err) => map_cli_error(ctx.log_format, err),
    }
}

async fn run_inner(ctx: &Ctx, args: Args) -> Result<i32, CliError> {
    let content = tokio::fs::read_to_string(&args.file)
        .await
        .map_err(|err| CliError::Other(format!("read {}: {err}", args.file.display())))?;
    let seeds = parse_seed_yaml(&content)?;
    if seeds.is_empty() {
        return Err(CliError::Other(
            "inventory seed has no hosts under all.hosts".into(),
        ));
    }

    if args.dry_run {
        emit_dry_run(ctx, &seeds, args.assign);
        return Ok(0);
    }

    let (_session, client) = authenticate(ctx, args.environment.as_deref(), None).await?;
    let mut rows = Vec::with_capacity(seeds.len());
    let mut failures = 0usize;

    for seed in &seeds {
        let mut row = ImportRow {
            hostname: seed.hostname.clone(),
            ansible_host: seed.ansible_host.clone(),
            role: seed.intended_role.clone(),
            declared: None,
            assigned_group: None,
            error: None,
        };
        match import_one(
            &client,
            seed,
            args.assign,
            &args.availability_group,
            args.cluster_number,
        )
        .await
        {
            Ok((declared, group)) => {
                row.declared = Some(declared);
                row.assigned_group = group;
            }
            Err(err) => {
                failures += 1;
                row.error = Some(err.to_string());
            }
        }
        rows.push(row);
    }

    emit_report(ctx, &rows);
    Ok(if failures > 0 { 1 } else { 0 })
}

async fn import_one(
    client: &Client,
    seed: &SeedHost,
    assign: bool,
    availability_group: &str,
    cluster_number: Option<i32>,
) -> Result<(DeclaredHost, Option<String>), CliError> {
    let declared = client
        .create_declared_host(&CreateDeclaredHostRequest {
            hostname: &seed.hostname,
            ansible_host: seed.ansible_host.as_deref(),
        })
        .await?;

    if !assign {
        return Ok((declared, None));
    }

    let role = seed.intended_role.as_deref().ok_or_else(|| {
        CliError::Other(format!(
            "host {} has no intended_inventory_role; omit --assign or set the role in the seed",
            seed.hostname
        ))
    })?;

    let cluster_number = if role_requires_cluster_number(role) {
        Some(cluster_number.unwrap_or(1))
    } else {
        cluster_number
    };

    let response = client
        .assign_inventory_host(&AssignHostRequest {
            host_id: declared.id,
            role: role.to_string(),
            availability_group: availability_group.to_string(),
            cluster_number,
        })
        .await?;

    Ok((declared, response.ansible_group_name))
}

fn emit_dry_run(ctx: &Ctx, seeds: &[SeedHost], assign: bool) {
    let payload = json!({
        "dry_run": true,
        "assign": assign,
        "hosts": seeds.iter().map(|h| json!({
            "hostname": h.hostname,
            "ansible_host": h.ansible_host,
            "intended_inventory_role": h.intended_role,
        })).collect::<Vec<_>>(),
    });
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        ),
        LogFormat::Human => {
            println!(
                "Dry run: {} host(s){}",
                seeds.len(),
                if assign { " (would assign)" } else { "" }
            );
            for host in seeds {
                let role = host.intended_role.as_deref().unwrap_or("-");
                let reach = host.ansible_host.as_deref().unwrap_or("-");
                println!("  {}  ansible_host={}  role={}", host.hostname, reach, role);
            }
        }
    }
}

fn emit_report(ctx: &Ctx, rows: &[ImportRow]) {
    let payload = json!({
        "hosts": rows.iter().map(|row| json!({
            "hostname": row.hostname,
            "ansible_host": row.ansible_host,
            "role": row.role,
            "hosts_row_id": row.declared.as_ref().map(|d| d.id),
            "host_id": row.declared.as_ref().map(|d| d.host_id),
            "ansible_group_name": row.assigned_group,
            "error": row.error,
        })).collect::<Vec<_>>(),
    });
    match ctx.log_format {
        LogFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        ),
        LogFormat::Human => {
            for row in rows {
                if let Some(err) = &row.error {
                    println!("{}  ERROR  {err}", row.hostname);
                    continue;
                }
                let declared = row.declared.as_ref().expect("success row has declared");
                match &row.assigned_group {
                    Some(group) => println!(
                        "{}  declared id={} host_id={}  assigned {}",
                        row.hostname, declared.id, declared.host_id, group
                    ),
                    None => println!(
                        "{}  declared id={} host_id={}",
                        row.hostname, declared.id, declared.host_id
                    ),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn clap_requires_file() {
        let mut cmd = crate::cli::Cli::command();
        let inventory = cmd.find_subcommand_mut("inventory").expect("inventory");
        let import = inventory
            .find_subcommand_mut("import-seed")
            .expect("import-seed");
        assert!(import
            .get_arguments()
            .any(|arg| arg.get_long() == Some("file")));
    }
}
