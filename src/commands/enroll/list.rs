use clap::Args as ClapArgs;

use crate::cli::LogFormat;
use crate::commands::pipeline::{authenticate, map_cli_error};
use crate::errors::CliError;
use crate::observer_client::EnrollmentTokenSummary;
use crate::Ctx;

const LIST_CAP: usize = 50;

#[derive(ClapArgs)]
pub struct Args {}

pub async fn run(ctx: Ctx, _args: Args) -> i32 {
    match run_inner(ctx).await {
        Ok(code) => code,
        Err(err) => map_cli_error(err),
    }
}

async fn run_inner(ctx: Ctx) -> Result<i32, CliError> {
    let (session, client) = authenticate(&ctx, None, None).await?;
    if !session.is_device_session() {
        return Err(CliError::Other(
            "`enroll list` requires `deslicer auth login` (device session)".into(),
        ));
    }

    let listed = client.list_enrollment_tokens().await?;
    let shown: Vec<&EnrollmentTokenSummary> = listed.tokens.iter().take(LIST_CAP).collect();

    match ctx.log_format {
        LogFormat::Json => {
            let payload = serde_json::json!({
                "tokens": shown,
                "shown": shown.len(),
                "total": listed.total,
            });
            let text = serde_json::to_string_pretty(&payload)
                .map_err(|err| CliError::Other(format!("serialize tokens: {err}")))?;
            assert_no_secret(&text)?;
            println!("{text}");
        }
        LogFormat::Human => {
            print!("{}", format_tokens_human(&shown, listed.total));
        }
    }
    Ok(0)
}

fn format_tokens_human(tokens: &[&EnrollmentTokenSummary], total: i64) -> String {
    if tokens.is_empty() {
        return "No enrollment tokens.\n".to_string();
    }
    let mut lines = vec!["JTI  PURPOSE  ACTIVE  EXPIRES  NAME".to_string()];
    for token in tokens {
        let name = token.name.as_deref().unwrap_or("-");
        let active = if token.is_active { "yes" } else { "no" };
        lines.push(format!(
            "{}  {}  {}  {}  {}",
            token.jti, token.token_purpose, active, token.expires_at, name
        ));
    }
    if total as usize > tokens.len() {
        lines.push(format!(
            "Showing {} of {total}. Use the portal for the full list.",
            tokens.len()
        ));
    }
    lines.push(String::new());
    let text = lines.join("\n");
    debug_assert!(!text.contains("dsle_enroll_"));
    text
}

fn assert_no_secret(text: &str) -> Result<(), CliError> {
    if text.contains("dsle_enroll_") {
        return Err(CliError::Other(
            "list response contained a secret token field; refusing to print".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample() -> EnrollmentTokenSummary {
        EnrollmentTokenSummary {
            jti: Uuid::parse_str("019f36d6-3f61-7eea-9417-7ac4a8a10f69").unwrap(),
            name: Some("fleet".into()),
            description: None,
            max_hosts: 10,
            enrolled_count: 0,
            expires_at: "2099-01-01T00:00:00Z".into(),
            created_at: "2026-08-24T00:00:00Z".into(),
            revoked_at: None,
            is_active: true,
            token_purpose: "bootstrap".into(),
            bind_to_host_id: None,
        }
    }

    #[test]
    fn human_list_never_contains_secret_prefix() {
        let token = sample();
        let text = format_tokens_human(&[&token], 1);
        assert!(text.contains("019f36d6-3f61-7eea-9417-7ac4a8a10f69"));
        assert!(text.contains("bootstrap"));
        assert!(!text.contains("dsle_enroll_"));
        assert!(!text.contains("token"));
    }

    #[test]
    fn json_guard_rejects_secret_prefix() {
        assert!(assert_no_secret(r#"{"jti":"x"}"#).is_ok());
        assert!(assert_no_secret(r#"{"token":"dsle_enroll_secret"}"#).is_err());
    }
}
