use std::ffi::OsStr;
use std::io::{self, Write};

use clap::error::ErrorKind;
use serde_json::{json, Value};

use crate::ci::OidcError;
use crate::cli::LogFormat;
use crate::errors::CliError;

/// Treat a closed pipe as success so agents piping into `head`/`jq` never panic.
fn ignore_broken_pipe(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}

fn write_stderr_line(line: &str) {
    let mut stderr = io::stderr();
    let _ = ignore_broken_pipe(writeln!(stderr, "{line}"));
}

const SECRET_ENV_VARS: &[&str] = &[
    "DESLICER_DEV_TOKEN", // pragma: allowlist secret
    "DESLICER_API_TOKEN", // pragma: allowlist secret
    "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
    "DESLICER_OIDC_TOKEN", // pragma: allowlist secret
    "CI_JOB_JWT",
    "BITBUCKET_STEP_OIDC_TOKEN",
    "SYSTEM_ACCESSTOKEN",
];

/// Infer `--log-format` from argv before clap parses (for usage errors).
pub fn log_format_from_args<I, S>(args: I) -> LogFormat
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut args = args
        .into_iter()
        .map(|s| s.as_ref().to_string_lossy().into_owned());
    while let Some(arg) = args.next() {
        if arg == "--json" {
            return LogFormat::Json;
        }
        if arg == "--log-format" {
            if let Some(value) = args.next() {
                return parse_log_format_value(&value);
            }
        } else if let Some(value) = arg.strip_prefix("--log-format=") {
            return parse_log_format_value(value);
        }
    }
    LogFormat::Human
}

fn parse_log_format_value(value: &str) -> LogFormat {
    match value.to_ascii_lowercase().as_str() {
        "json" => LogFormat::Json,
        _ => LogFormat::Human,
    }
}

/// Remove secret material from free-form text before it is printed.
pub fn redact_secrets(text: &str) -> String {
    let mut redacted = text.to_string();
    for name in SECRET_ENV_VARS {
        if let Ok(value) = std::env::var(name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                redacted = redacted.replace(trimmed, "***");
            }
        }
    }
    redact_bearer_tokens(&mut redacted);
    redacted
}

fn redact_bearer_tokens(text: &mut String) {
    let marker = "Bearer ";
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find(marker) {
        let start = search_from + rel + marker.len();
        let end = text[start..]
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == ']')
            .map(|idx| start + idx)
            .unwrap_or(text.len());
        if end > start {
            text.replace_range(start..end, "***");
        }
        search_from = start + 3;
    }
}

pub fn cli_error_kind(err: &CliError) -> &'static str {
    match err {
        CliError::OidcRejected(_) => "oidc",
        CliError::ApiTokenRejected(_) => "api_token",
        CliError::RepoNotAllowlisted(_) => "auth",
        CliError::EnvironmentNotBound(_) => "auth",
        CliError::AmbiguousBinding(_) | CliError::GitBackedChangeManagement(_) => "auth",
        CliError::UnsupportedPlatform(_) => "platform",
        CliError::InvalidInput(_) => "usage",
        CliError::RateLimited { .. } => "rate_limit",
        CliError::BackendUnavailable(_) => "transport",
        CliError::Transport(_) => "transport",
        CliError::PlanNotFound(_) => "plan",
        CliError::EndpointNotAvailable(_) => "endpoint",
        CliError::HumanApprovalRequired(_) => "approval",
        CliError::AgentRunFailed(_) => "agent",
        CliError::ValidationModelUnavailable
        | CliError::ValidationTimeout
        | CliError::ValidationUnavailable => "validation",
        CliError::Other(msg) if msg.contains("not logged in") || msg.contains("OIDC") => "auth",
        CliError::Other(_) => "error",
    }
}

pub fn cli_error_json(err: &CliError) -> Value {
    let message = redact_secrets(&err.to_string());
    let mut body = json!({
        "kind": cli_error_kind(err),
        "message": message,
    });
    match err {
        CliError::Other(msg) if msg.contains("not logged in") => {
            body["hint"] = json!("run auth login and approve the code in the portal");
        }
        CliError::ApiTokenRejected(_) => {
            body["hint"] = json!(
                "mint a tools-scope DESLICER_API_TOKEN in the portal, or run deslicer auth login"
            );
        }
        _ => {}
    }
    json!({ "error": body })
}

pub fn oidc_error_json(err: &OidcError) -> Value {
    let message = redact_secrets(&err.to_string());
    json!({
        "error": {
            "kind": "oidc",
            "message": message,
        }
    })
}

pub fn oidc_exit_code(err: &OidcError) -> i32 {
    match err {
        OidcError::MissingEnv(_) => 4,
        OidcError::Unsupported(_) => 8,
        OidcError::Http(_) => 10,
        OidcError::Other(_) => 1,
    }
}

pub fn emit_cli_error(format: LogFormat, err: &CliError) -> i32 {
    let code = err.exit_code();
    match format {
        LogFormat::Json => {
            write_stderr_line(&serde_json::to_string(&cli_error_json(err)).unwrap_or_default());
        }
        LogFormat::Human => {
            write_stderr_line(&redact_secrets(&err.to_string()));
        }
    }
    code
}

pub fn emit_oidc_error(format: LogFormat, err: &OidcError) -> i32 {
    let code = oidc_exit_code(err);
    match format {
        LogFormat::Json => {
            write_stderr_line(&serde_json::to_string(&oidc_error_json(err)).unwrap_or_default());
        }
        LogFormat::Human => {
            write_stderr_line(&redact_secrets(&err.to_string()));
        }
    }
    code
}

pub fn emit_clap_error(format: LogFormat, err: &clap::Error) -> i32 {
    let code = match err.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => 0,
        ErrorKind::DisplayVersion => 0,
        _ => 2,
    };
    match format {
        LogFormat::Json => {
            let message = redact_secrets(&err.to_string());
            write_stderr_line(
                &serde_json::to_string(&json!({
                    "error": {
                        "kind": "usage",
                        "message": message,
                    }
                }))
                .unwrap_or_default(),
            );
        }
        LogFormat::Human => {
            // clap writes help/usage to stdout; a closed pipe must not panic (agents use head/jq).
            if let Err(print_err) = ignore_broken_pipe(err.print()) {
                write_stderr_line(&format!("clap error print failed: {print_err}"));
            }
        }
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn log_format_from_args_reads_flag() {
        let args = ["cli", "--log-format", "json", "auth", "status"];
        assert_eq!(log_format_from_args(args), LogFormat::Json);
    }

    #[test]
    fn log_format_from_args_reads_equals_form() {
        let args = ["cli", "auth", "status", "--log-format=json"];
        assert_eq!(log_format_from_args(args), LogFormat::Json);
    }

    #[test]
    fn redact_secrets_masks_env_token() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("DESLICER_DEV_TOKEN", "super-secret-dev-token"); // pragma: allowlist secret
        let redacted = redact_secrets("token super-secret-dev-token leaked");
        assert!(!redacted.contains("super-secret-dev-token"));
        assert!(redacted.contains("***"));
        std::env::remove_var("DESLICER_DEV_TOKEN"); // pragma: allowlist secret
    }

    #[test]
    fn redact_secrets_masks_bearer_tokens() {
        let redacted = redact_secrets("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload");
        assert!(!redacted.contains("eyJhbGciOiJIUzI1NiJ9"));
        assert!(redacted.contains("Bearer ***"));
    }

    #[test]
    fn cli_error_json_has_kind_and_message() {
        let err = CliError::Other("not logged in".into());
        let value = cli_error_json(&err);
        assert_eq!(value["error"]["kind"], "auth");
        assert!(value["error"]["hint"].is_string());
    }

    #[test]
    fn api_token_rejected_json_is_not_oidc() {
        let err = CliError::ApiTokenRejected("HTTP 401 Unauthorized".into());
        let value = cli_error_json(&err);
        assert_eq!(value["error"]["kind"], "api_token");
        assert!(value["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("DESLICER_API_TOKEN"));
        assert!(!value["error"]["message"].as_str().unwrap().contains("OIDC"));
    }

    #[test]
    fn ignore_broken_pipe_swallows_epipe_only() {
        assert!(ignore_broken_pipe(Ok(())).is_ok());
        assert!(ignore_broken_pipe(Err(io::Error::from(io::ErrorKind::BrokenPipe))).is_ok());
        let err = ignore_broken_pipe(Err(io::Error::from(io::ErrorKind::PermissionDenied)));
        assert_eq!(err.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }
}
