//! `deslicer inventory validate` — fail-closed env YAML checks for thin PR CI.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;
use serde::Serialize;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::environment_paths::{resolve_environment_stem, search_roots_for, ResolvedStem};
use crate::environment_yaml::{
    resolve_env_file, validate_environment_yaml, Severity, ValidationIssue, ValidationReport,
};
use crate::errors::CliError;
use crate::token_store::load_active_session;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Environment filename stem (GitHub Environment / tenant slug).
    #[arg(long)]
    pub environment: Option<String>,

    /// Repository root that contains `.deslicer/environments/` (default: `.`).
    #[arg(long)]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct JsonReport {
    ok: bool,
    file: String,
    errors: usize,
    warnings: usize,
    issues: Vec<ValidationIssue>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    match run_inner(&ctx, args).await {
        Ok(report) if report.is_ok() => {
            emit_report(&ctx, &report);
            0
        }
        Ok(report) => {
            emit_report(&ctx, &report);
            1
        }
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: &Ctx, args: Args) -> Result<ValidationReport, CliError> {
    let dir = resolve_dir(args.dir.as_deref())?;
    let resolved = resolve_stem(&dir, args.environment.as_deref())?;
    let path = resolve_env_file(&dir, &resolved.stem).map_err(CliError::Other)?;
    let content = std::fs::read_to_string(&path)
        .map_err(|err| CliError::Other(format!("read {}: {err}", path.display())))?;

    let (_session, client) = authenticate(ctx, args.environment.as_deref(), None).await?;
    let groups = client.list_groups().await?;
    let known: HashSet<String> = groups.into_iter().map(|group| group.name).collect();

    let label = path
        .strip_prefix(&dir)
        .unwrap_or(&path)
        .display()
        .to_string();
    Ok(validate_environment_yaml(
        &content,
        &label,
        &dir,
        Some(&known),
    ))
}

fn resolve_dir(dir: Option<&Path>) -> Result<PathBuf, CliError> {
    let dir = dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    if !dir.is_dir() {
        return Err(CliError::Other(format!(
            "--dir {} is not a directory",
            dir.display()
        )));
    }
    Ok(dir)
}

fn resolve_stem(dir: &Path, explicit: Option<&str>) -> Result<ResolvedStem, CliError> {
    let tenant_slug = load_active_session()?.and_then(|session| session.tenant_slug);
    let roots = search_roots_for(dir);
    let refs: Vec<&Path> = roots.iter().map(|path| path.as_path()).collect();
    resolve_environment_stem(explicit, tenant_slug.as_deref(), &refs)
}

fn emit_report(ctx: &Ctx, report: &ValidationReport) {
    match ctx.log_format {
        LogFormat::Json => println!("{}", json_report(report)),
        LogFormat::Human => print!("{}", human_report(report)),
    }
}

fn json_report(report: &ValidationReport) -> serde_json::Value {
    let errors = report.errors().count();
    let warnings = report.warnings().count();
    serde_json::to_value(JsonReport {
        ok: report.is_ok(),
        file: report.file.clone(),
        errors,
        warnings,
        issues: report.issues.clone(),
    })
    .unwrap_or_else(|_| serde_json::json!({"ok": false}))
}

fn human_report(report: &ValidationReport) -> String {
    let mut lines = Vec::new();
    let error_count = report.errors().count();
    let warning_count = report.warnings().count();

    if report.issues.is_empty() {
        lines.push(format!("inventory validate: {} OK", report.file));
        lines.push(String::new());
        return lines.join("\n");
    }

    for issue in &report.issues {
        let marker = match issue.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        lines.push(format!(
            "inventory validate: {marker} {}:{}: {}",
            issue.file, issue.path, issue.message
        ));
        lines.push(format!("  suggestion: {}", issue.suggestion));
    }

    if error_count > 0 {
        lines.push(format!(
            "inventory validate: failed — {error_count} error(s), {warning_count} warning(s)"
        ));
    } else {
        lines.push(format!(
            "inventory validate: {} OK with {warning_count} warning(s)",
            report.file
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment_yaml::ValidationIssue;

    fn sample_report() -> ValidationReport {
        ValidationReport {
            file: ".deslicer/environments/prod.yml".into(),
            issues: vec![ValidationIssue {
                file: ".deslicer/environments/prod.yml".into(),
                path: "destinations[0].inventory_group".into(),
                severity: Severity::Error,
                message: "unknown inventory_group \"ghost\"".into(),
                suggestion: "Use an exact host-group name from `deslicer groups list`".into(),
            }],
        }
    }

    #[test]
    fn human_report_lists_errors_and_suggestions() {
        let text = human_report(&sample_report());
        assert!(text.contains("error"));
        assert!(text.contains("unknown inventory_group"));
        assert!(text.contains("suggestion:"));
        assert!(text.contains("failed"));
    }

    #[test]
    fn json_report_marks_not_ok() {
        let value = json_report(&sample_report());
        assert_eq!(value["ok"], false);
        assert_eq!(value["errors"], 1);
        assert_eq!(
            value["issues"][0]["path"],
            "destinations[0].inventory_group"
        );
    }

    #[test]
    fn human_report_ok_is_quiet() {
        let report = ValidationReport {
            file: "prod.yml".into(),
            issues: vec![],
        };
        let text = human_report(&report);
        assert!(text.contains("OK"));
        assert!(!text.contains("failed"));
    }
}
