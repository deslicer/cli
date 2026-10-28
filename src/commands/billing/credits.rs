use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::billing::client::{BillingClient, BillingCredits};
use crate::commands::pipeline::map_cli_error;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    let client = match BillingClient::from_ctx(&ctx) {
        Ok(client) => client,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    let credits = match client.credits().await {
        Ok(credits) => credits,
        Err(err) => return map_cli_error(ctx.log_format, err),
    };
    match ctx.log_format {
        LogFormat::Json => match serde_json::to_string_pretty(&credits) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(err) => {
                eprintln!("failed to serialize credits: {err}");
                1
            }
        },
        LogFormat::Human => {
            print!("{}", format_credits_human(&credits));
            0
        }
    }
}

fn format_credits_human(credits: &BillingCredits) -> String {
    let mut lines = Vec::new();
    if let Some(balance) = credits.balance {
        let currency = credits.currency.as_deref().unwrap_or("");
        lines.push(format!("balance: {balance} {currency}").trim().to_string());
    }
    if let Some(exp) = credits.expires_at.as_deref() {
        lines.push(format!("expires_at: {exp}"));
    }
    if let Some(msg) = credits.message.as_deref() {
        lines.push(msg.to_string());
    }
    if lines.is_empty() {
        lines.push("no credits reported".to_string());
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_credits_rows() {
        let text = format_credits_human(&BillingCredits {
            balance: Some(100.0),
            currency: Some("credits".into()),
            expires_at: None,
            message: None,
        });
        assert!(text.contains("100"));
    }
}
