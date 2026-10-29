use clap::Args as ClapArgs;

use super::{output_format, print_json};
use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::{resolve_targets, UpgradeReadiness};
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {
    /// Host to check (hostname or host UUID)
    #[arg(long, required = true)]
    pub target: String,

    #[arg(long)]
    pub environment: Option<String>,

    /// Print JSON (same as --log-format json)
    #[arg(long)]
    pub json: bool,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let format = output_format(&ctx, args.json);
    match run_inner(&ctx, &args, format).await {
        Ok(code) => code,
        Err(err) => map_cli_error(format, err),
    }
}

async fn run_inner(ctx: &Ctx, args: &Args, format: LogFormat) -> Result<i32, CliError> {
    let (_session, client) = authenticate(ctx, args.environment.as_deref(), None).await?;
    let hosts = client.list_enrolled_hosts().await?;
    let host_id = resolve_targets(&hosts, std::slice::from_ref(&args.target))?.remove(0);
    let report = client.upgrade_readiness(&host_id).await?;
    Ok(match format {
        LogFormat::Json => print_json(&serde_json::json!({
            "target": args.target,
            "host_id": host_id,
            "readiness": report,
        })),
        LogFormat::Human => {
            print!("{}", format_human(&args.target, &report));
            0
        }
    })
}

fn format_human(target: &str, r: &UpgradeReadiness) -> String {
    let mut lines = vec![
        format!("target: {target}"),
        format!("path: {}", r.path),
        format!(
            "installed_version: {}",
            r.installed_version.as_deref().unwrap_or("-")
        ),
        format!("pin_version: {}", r.pin_version.as_deref().unwrap_or("-")),
        format!(
            "captain_stability: {}",
            r.captain_stability.as_deref().unwrap_or("unknown")
        ),
        format!("needs_readiness_plan: {}", r.needs_readiness_plan),
    ];
    if r.strict_x509_required {
        lines.push(format!(
            "strict_x509: {}",
            match r.strict_x509_passed {
                Some(true) => "passed",
                Some(false) => "failed",
                None => "not applicable",
            }
        ));
    }
    for reason in &r.reasons {
        lines.push(format!("reason: {reason}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_shows_captain_stability() {
        let r: UpgradeReadiness = serde_json::from_value(serde_json::json!({
            "path": "upgrade", "needs_readiness_plan": true, "reasons": ["pin ahead"],
            "pin_version": "9.4.1", "installed_version": "9.2.0",
            "strict_x509_required": true, "strict_x509_passed": null,
            "captain_stability": "flapping"
        }))
        .unwrap();
        let text = format_human("sh1", &r);
        assert!(text.contains("captain_stability: flapping"));
        assert!(text.contains("strict_x509: not applicable"));
        assert!(text.contains("reason: pin ahead"));
    }
}
