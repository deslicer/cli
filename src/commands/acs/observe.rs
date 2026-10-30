use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::observer_client::AcsObserveReport;
use crate::Ctx;

/// Read-only scan of a Splunk Cloud stack through ACS. Never applies changes.
///
/// Examples:
///   deslicer acs observe my-stack
///   deslicer --log-format json acs observe my-stack --experience classic
#[derive(ClapArgs)]
pub struct Args {
    /// ACS stack name (from `acs stacks list`)
    pub stack: String,

    /// Splunk Cloud experience: victoria (default on Observer) or classic
    #[arg(long, value_parser = ["victoria", "classic"])]
    pub experience: Option<String>,

    /// Deslicer environment profile used for auth
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(ctx: Ctx, args: Args) -> i32 {
    let (_session, client) = match authenticate(&ctx, args.environment.as_deref(), None).await {
        Ok(pair) => pair,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    let report = match client
        .observe_acs_stack(&args.stack, args.experience.as_deref())
        .await
    {
        Ok(report) => report,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };

    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(&report) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize ACS observe report: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_report_human(&report));
            eprintln!(
                "Observe never applies changes. Create a Pending plan with:\n  \
                 deslicer change plan --destination acs-stack:{}",
                args.stack
            );
            0
        }
    }
}

fn format_report_human(report: &AcsObserveReport) -> String {
    let mut lines = vec![format!(
        "ACS observe (read-only) for stack {}: {} object(s)",
        report.stack, report.object_count
    )];
    if !report.objects.is_empty() {
        lines.push("TYPE  ID".to_string());
        for object in &report.objects {
            lines.push(format!("{}  {}", object.resource_type, object.resource_id));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observer_client::AcsObservedObject;

    #[test]
    fn parses_observer_response_shape() {
        let report: AcsObserveReport = serde_json::from_str(
            r#"{"stack_name":"acme","object_count":1,"objects":[{"resource_type":"index","resource_id":"main","content_hash":"abc"}]}"#,
        )
        .unwrap();
        assert_eq!(report.stack, "acme");
        let text = format_report_human(&report);
        assert!(text.contains("1 object(s)"));
        assert!(text.contains("index  main"));
        let _ = AcsObservedObject {
            resource_type: "x".into(),
            resource_id: "y".into(),
            content_hash: None,
        };
    }
}
