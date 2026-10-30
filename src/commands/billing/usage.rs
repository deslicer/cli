use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::billing::client::{BillingClient, BillingUsage};
use crate::commands::pipeline::map_cli_error;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    let client = match BillingClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let usage = match client.usage().await {
        Ok(usage) => usage,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match crate::json_doc::to_pretty_string(&usage) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize usage: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_usage_human(&usage));
            0
        }
    }
}

fn format_usage_human(usage: &BillingUsage) -> String {
    let mut lines = Vec::new();
    match (usage.period_start.as_deref(), usage.period_end.as_deref()) {
        (Some(s), Some(e)) => lines.push(format!("period: {s} → {e}")),
        (Some(s), None) => lines.push(format!("period_start: {s}")),
        (None, Some(e)) => lines.push(format!("period_end: {e}")),
        _ => {}
    }
    if let Some(units) = usage.units {
        let label = usage.unit_label.as_deref().unwrap_or("units");
        lines.push(format!("usage: {units} {label}"));
    }
    if let Some(cost) = usage.cost {
        let currency = usage.currency.as_deref().unwrap_or("");
        lines.push(format!("cost: {cost} {currency}").trim().to_string());
    }
    if let Some(breakdown) = usage.breakdown.as_ref() {
        if let Ok(json) = crate::json_doc::to_pretty_string(breakdown) {
            lines.push(json);
        }
    }
    if lines.is_empty() {
        lines.push("no usage reported".to_string());
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_usage_rows() {
        let text = format_usage_human(&BillingUsage {
            period_start: Some("2026-10-01".into()),
            period_end: Some("2026-10-31".into()),
            units: Some(12.5),
            unit_label: Some("credits".into()),
            cost: Some(3.0),
            currency: Some("USD".into()),
            breakdown: None,
        });
        assert!(text.contains("12.5 credits"));
        assert!(text.contains("USD"));
    }
}
