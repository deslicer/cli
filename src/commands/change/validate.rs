use clap::{Args as ClapArgs, ValueEnum};

use crate::commands::pipeline::{authenticate, map_cli_error, require_proxy_mode};
use crate::observer_client::{PlanValidationFinding, PlanValidationReport};
use crate::Ctx;

const EXIT_BLOCKED: i32 = 20;
const EXIT_WARNING: i32 = 21;
const EXIT_UNAVAILABLE: i32 = 24;

#[derive(ClapArgs)]
pub struct Args {
    /// External plan id returned by `deslicer change plan`.
    #[arg(long)]
    pub plan_id: String,

    #[arg(long)]
    pub environment: Option<String>,

    /// Re-run validation even when the current report is still effective.
    #[arg(long)]
    pub force: bool,

    /// Fetch the current report without triggering validation.
    #[arg(long)]
    pub report_only: bool,

    /// Report output written to stdout.
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    /// Finding level that makes the command exit non-zero.
    #[arg(long, value_enum, default_value_t = FailOn::Block)]
    pub fail_on: FailOn,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
    Markdown,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum FailOn {
    Block,
    Warning,
    Never,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (session, client) =
        match authenticate(&ctx, args.environment.as_deref(), Some(&args.plan_id)).await {
            Ok(pair) => pair,
            Err(err) => return map_cli_error(ctx.log_format, err),
        };

    let report = if args.report_only && session.is_observer_api_token() {
        client.get_plan_validation_direct(&args.plan_id).await
    } else {
        if let Err(err) = require_proxy_mode(&session, "change validate") {
            return map_cli_error(ctx.log_format, err);
        }
        if args.report_only {
            client.get_plan_validation(&args.plan_id).await
        } else {
            client.validate_plan(&args.plan_id, args.force).await
        }
    };
    let report = match report {
        Ok(report) => report,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    emit_report(args.format, &report);
    report_exit_code(&report, args.fail_on)
}

fn emit_report(format: OutputFormat, report: &PlanValidationReport) {
    match format {
        OutputFormat::Human => println!("{}", human_report(report)),
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(report).unwrap_or_default());
        }
        OutputFormat::Markdown => println!("{}", markdown_report(report)),
    }
}

fn human_report(report: &PlanValidationReport) -> String {
    let (errors, warnings, info) = finding_counts(report);
    let mut lines = vec![
        format!("Plan validation: {}", report.verdict.to_ascii_uppercase()),
        format!(
            "{} error(s), {} warning(s), {} info finding(s)",
            errors, warnings, info
        ),
    ];
    for finding in &report.findings {
        lines.push(format!(
            "- [{}] {}: {} ({})",
            finding.severity.to_ascii_uppercase(),
            finding_location(finding),
            single_line(&finding.message),
            finding.code
        ));
    }
    lines.join("\n")
}

fn markdown_report(report: &PlanValidationReport) -> String {
    let (errors, warnings, info) = finding_counts(report);
    let mut lines = vec![
        format!(
            "### Deslicer plan validation: {}",
            title_case(&report.verdict)
        ),
        String::new(),
        format!(
            "{} error(s), {} warning(s), {} info finding(s).",
            errors, warnings, info
        ),
    ];
    if !report.findings.is_empty() {
        lines.extend([
            String::new(),
            "| Severity | Location | Finding |".into(),
            "|---|---|---|".into(),
        ]);
        lines.extend(report.findings.iter().map(|finding| {
            format!(
                "| {} | `{}` | {} (`{}`) |",
                escape_markdown(&finding.severity),
                escape_markdown(&finding_location(finding)),
                escape_markdown(&finding.message),
                escape_markdown(&finding.code)
            )
        }));
    }
    lines.join("\n")
}

fn report_exit_code(report: &PlanValidationReport, fail_on: FailOn) -> i32 {
    match report.verdict.as_str() {
        "pass" => 0,
        "warn" if matches!(fail_on, FailOn::Warning) => EXIT_WARNING,
        "warn" => 0,
        "block" if matches!(fail_on, FailOn::Never) => 0,
        "block" => EXIT_BLOCKED,
        "error" => EXIT_UNAVAILABLE,
        _ => EXIT_UNAVAILABLE,
    }
}

fn finding_counts(report: &PlanValidationReport) -> (usize, usize, usize) {
    report
        .findings
        .iter()
        .fold((0, 0, 0), |mut counts, finding| {
            match finding.severity.as_str() {
                "error" => counts.0 += 1,
                "warning" => counts.1 += 1,
                _ => counts.2 += 1,
            }
            counts
        })
}

fn finding_location(finding: &PlanValidationFinding) -> String {
    let mut location = finding.config_path.clone();
    if let Some(stanza) = finding.stanza.as_deref() {
        location.push_str(&format!(" [{stanza}]"));
    }
    if let Some(key) = finding.key.as_deref() {
        location.push_str(&format!(" {key}"));
    }
    location
}

fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn escape_markdown(value: &str) -> String {
    single_line(value).replace('|', "\\|").replace('`', "\\`")
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(verdict: &str, severity: &str) -> PlanValidationReport {
        PlanValidationReport {
            plan_id: "01994bdb-2d78-79d5-8f40-c03d342a3bc1".into(),
            items_sha256: "digest".into(),
            unsigned_plan_sha256: None,
            verdict: verdict.into(),
            effective: true,
            overridden: false,
            report: serde_json::json!({}),
            findings: vec![PlanValidationFinding {
                app_name: Some("search".into()),
                config_path: "local/web.conf".into(),
                stanza: Some("settings".into()),
                key: Some("enableSplunkWebSSL".into()),
                severity: severity.into(),
                code: "invalid_value".into(),
                message: "expected true | false".into(),
                spec_reference: None,
                suggested_value: Some("true".into()),
            }],
            engine: "dap-agent".into(),
            engine_version: "1".into(),
            model_id: None,
            duration_ms: Some(20),
            created_at: "2026-09-22T07:00:00Z".into(),
            override_reason: None,
        }
    }

    #[test]
    fn exit_contract_distinguishes_block_warning_and_unavailable() {
        assert_eq!(
            report_exit_code(&report("block", "error"), FailOn::Block),
            20
        );
        assert_eq!(
            report_exit_code(&report("warn", "warning"), FailOn::Block),
            0
        );
        assert_eq!(
            report_exit_code(&report("warn", "warning"), FailOn::Warning),
            21
        );
        assert_eq!(
            report_exit_code(&report("error", "error"), FailOn::Never),
            24
        );
    }

    #[test]
    fn markdown_is_deterministic_and_escapes_table_content() {
        let output = markdown_report(&report("block", "error"));
        assert!(output.contains("### Deslicer plan validation: Block"));
        assert!(output.contains("expected true \\| false"));
        assert!(output.contains("`local/web.conf [settings] enableSplunkWebSSL`"));
    }
}
