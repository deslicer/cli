use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::LicenseStatus;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let status = match client.license_status().await {
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
                eprintln!("failed to serialize license status: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_status_human(&status));
            0
        }
    }
}

fn format_status_human(status: &LicenseStatus) -> String {
    let mut lines = Vec::new();
    if let Some(s) = status.status.as_deref() {
        lines.push(format!("status: {s}"));
    }
    if let Some(plan) = status.plan.as_deref() {
        lines.push(format!("plan: {plan}"));
    }
    match (status.seats_used, status.seats) {
        (Some(used), Some(total)) => lines.push(format!("seats: {used}/{total}")),
        (None, Some(total)) => lines.push(format!("seats: {total}")),
        (Some(used), None) => lines.push(format!("seats_used: {used}")),
        _ => {}
    }
    if let Some(exp) = status.expires_at.as_deref() {
        lines.push(format!("expires_at: {exp}"));
    }
    if let Some(bound) = status.bound {
        lines.push(format!("bound: {}", if bound { "yes" } else { "no" }));
    }
    if let Some(hint) = status.license_key_hint.as_deref() {
        lines.push(format!("key: {hint}"));
    }
    if let Some(msg) = status.message.as_deref() {
        lines.push(format!("message: {msg}"));
    }
    if lines.is_empty() {
        lines.push("no license status reported".to_string());
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_status_rows() {
        let text = format_status_human(&LicenseStatus {
            status: Some("active".into()),
            plan: Some("pro".into()),
            seats: Some(10),
            seats_used: Some(3),
            expires_at: None,
            bound: Some(true),
            license_key_hint: Some("****abcd".into()),
            message: None,
        });
        assert!(text.contains("active"));
        assert!(text.contains("3/10"));
        assert!(text.contains("bound: yes"));
    }
}
