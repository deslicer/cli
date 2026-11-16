//! `app-state fingerprint|publish|fetch` against a local directory store.

use std::path::PathBuf;

use clap::{Args as ClapArgs, Subcommand};
use serde_json::json;

use crate::cli::LogFormat;
use crate::errors::CliError;
use crate::Ctx;

#[derive(Subcommand)]
pub enum AppStateCmd {
    /// Hash an app directory. Does not write.
    Fingerprint(DirArgs),
    /// Store immutable objects for an app directory (dry-run unless --yes).
    Publish(PublishArgs),
    /// Read one object back by digest (dry-run unless --yes and --output).
    Fetch(FetchArgs),
}

#[derive(ClapArgs)]
pub struct DirArgs {
    /// App directory to hash.
    #[arg(long)]
    pub dir: PathBuf,
}

#[derive(ClapArgs)]
pub struct PublishArgs {
    /// App directory to store.
    #[arg(long)]
    pub dir: PathBuf,
    /// Local object-store directory.
    #[arg(long)]
    pub store: PathBuf,
    /// Write objects. Without this flag, print the fingerprint and exit.
    #[arg(long)]
    pub yes: bool,
}

#[derive(ClapArgs)]
pub struct FetchArgs {
    /// Local object-store directory.
    #[arg(long)]
    pub store: PathBuf,
    /// SHA-256 digest of the object.
    #[arg(long)]
    pub digest: String,
    /// File to write. Requires --yes.
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Write --output. Without this flag, print the size and link only.
    #[arg(long)]
    pub yes: bool,
}

pub fn dispatch(ctx: Ctx, cmd: AppStateCmd) -> i32 {
    let format = ctx.log_format;
    let result = match cmd {
        AppStateCmd::Fingerprint(args) => fingerprint(&args),
        AppStateCmd::Publish(args) => publish(&args),
        AppStateCmd::Fetch(args) => fetch(&args),
    };
    match result {
        Ok(value) => match format {
            LogFormat::Json => crate::json_doc::print_success(&value),
            LogFormat::Human => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).unwrap_or_default()
                );
                0
            }
        },
        Err(err) => crate::reporting::emit_cli_error(format, &err),
    }
}

fn fingerprint(args: &DirArgs) -> Result<serde_json::Value, CliError> {
    let fp = crate::app_state::fingerprint::fingerprint_dir(&args.dir)
        .map_err(CliError::InvalidInput)?;
    Ok(json!({
        "fingerprint": fp.fingerprint,
        "files": fp.files.iter().map(|file| json!({
            "path": file.path,
            "sha256": file.sha256,
            "bytes": file.bytes,
        })).collect::<Vec<_>>(),
    }))
}

fn publish(args: &PublishArgs) -> Result<serde_json::Value, CliError> {
    if !args.yes {
        let fp = crate::app_state::fingerprint::fingerprint_dir(&args.dir)
            .map_err(CliError::InvalidInput)?;
        return Ok(json!({
            "dry_run": true,
            "fingerprint": fp.fingerprint,
            "file_count": fp.files.len(),
            "store": args.store.display().to_string(),
        }));
    }
    let published =
        crate::app_state::store::publish(&args.dir, &args.store).map_err(CliError::InvalidInput)?;
    Ok(json!({
        "dry_run": false,
        "fingerprint": published.fingerprint,
        "objects": published.objects.iter().map(|object| json!({
            "path": object.path,
            "sha256": object.sha256,
            "bytes": object.bytes,
            "link": object.link,
            "already_present": object.already_present,
        })).collect::<Vec<_>>(),
    }))
}

fn fetch(args: &FetchArgs) -> Result<serde_json::Value, CliError> {
    let bytes = crate::app_state::store::fetch(&args.store, &args.digest)
        .map_err(CliError::InvalidInput)?;
    if args.yes {
        let output = args.output.as_ref().ok_or_else(|| {
            CliError::InvalidInput("--yes requires --output for app-state fetch".into())
        })?;
        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    CliError::Other(format!("create {}: {err}", parent.display()))
                })?;
            }
        }
        std::fs::write(output, &bytes)
            .map_err(|err| CliError::Other(format!("write {}: {err}", output.display())))?;
    }
    Ok(json!({
        "dry_run": !args.yes,
        "sha256": args.digest,
        "bytes": bytes.len(),
        "wrote": args.yes,
    }))
}
