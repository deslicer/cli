use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Optional Splunk package name filter
    #[arg(long)]
    pub package: Option<String>,

    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let status = match client.upgrade_status(args.package.as_deref()).await {
        Ok(status) => status,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&status) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize upgrade status: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_status_human(&status));
            0
        }
    }
}

fn format_status_human(status: &crate::observer_client::UpgradeStatus) -> String {
    let mut lines = Vec::new();
    if let Some(s) = status.status.as_deref() {
        lines.push(format!("status: {s}"));
    }
    match (status.flapping_gate.as_deref(), status.flapping_gate_ok) {
        (Some(gate), Some(ok)) => lines.push(format!(
            "flapping_gate: {gate} ({})",
            if ok { "ok" } else { "blocked" }
        )),
        (Some(gate), None) => lines.push(format!("flapping_gate: {gate}")),
        (None, Some(ok)) => lines.push(format!(
            "flapping_gate: {}",
            if ok { "ok" } else { "blocked" }
        )),
        _ => {}
    }
    if let Some(v) = status.current_version.as_deref() {
        lines.push(format!("current_version: {v}"));
    }
    if let Some(v) = status.target_version.as_deref() {
        lines.push(format!("target_version: {v}"));
    }
    if let Some(p) = status.package.as_deref() {
        lines.push(format!("package: {p}"));
    }
    if let Some(m) = status.message.as_deref() {
        lines.push(format!("message: {m}"));
    }
    if lines.is_empty() {
        lines.push("no upgrade status reported".to_string());
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observer_client::UpgradeStatus;

    #[test]
    fn shows_flapping_gate() {
        let text = format_status_human(&UpgradeStatus {
            status: Some("ready".into()),
            flapping_gate: Some("stable".into()),
            flapping_gate_ok: Some(true),
            current_version: Some("9.1".into()),
            target_version: Some("9.2".into()),
            package: None,
            message: None,
            detail: None,
        });
        assert!(text.contains("flapping_gate: stable (ok)"));
        assert!(text.contains("9.2"));
    }
}
