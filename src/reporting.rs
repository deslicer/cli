use std::ffi::OsStr;
use std::io::{self, IsTerminal, Write};

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

/// Conflicting `--output` / `--json` / `--log-format` / `DESLICER_OUTPUT` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputFormatConflict;

/// Resolve the effective output format from explicit sources + TTY default.
///
/// Default is JSON when stdout is **not** a TTY (not keyed off `CI=true`).
pub fn resolve_output_format(
    output: Option<LogFormat>,
    json_flag: bool,
    log_format: Option<LogFormat>,
    stdout_is_tty: bool,
) -> Result<LogFormat, OutputFormatConflict> {
    let mut selected: Option<LogFormat> = None;
    let mut assign = |next: LogFormat| -> Result<(), OutputFormatConflict> {
        if let Some(prev) = selected {
            if prev != next {
                return Err(OutputFormatConflict);
            }
        }
        selected = Some(next);
        Ok(())
    };
    if json_flag {
        assign(LogFormat::Json)?;
    }
    if let Some(value) = output {
        assign(value)?;
    }
    if let Some(value) = log_format {
        assign(value)?;
    }
    Ok(selected.unwrap_or(if stdout_is_tty {
        LogFormat::Human
    } else {
        LogFormat::Json
    }))
}

/// Infer output format from argv (+ `DESLICER_OUTPUT`) before clap parses.
///
/// Used so clap usage errors honour `--output`/`--json`/`--log-format` and the
/// non-TTY JSON default. On conflict, returns [`OutputFormatConflict`].
pub fn output_format_from_args<I, S>(args: I) -> Result<LogFormat, OutputFormatConflict>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut output = None;
    let mut json_flag = false;
    let mut log_format = None;

    let mut args = args
        .into_iter()
        .map(|s| s.as_ref().to_string_lossy().into_owned());
    while let Some(arg) = args.next() {
        if arg == "--json" {
            json_flag = true;
            continue;
        }
        if arg == "--output" || arg == "-o" {
            if let Some(value) = args.next() {
                output = Some(parse_log_format_value(&value));
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--output=") {
            output = Some(parse_log_format_value(value));
            continue;
        }
        // Glued short form: only `-ojson` / `-ohuman` (not e.g. `-ops`).
        if let Some(value) = arg.strip_prefix("-o") {
            match value.to_ascii_lowercase().as_str() {
                "json" | "human" => {
                    output = Some(parse_log_format_value(value));
                    continue;
                }
                _ => {}
            }
        }
        if arg == "--log-format" {
            if let Some(value) = args.next() {
                log_format = Some(parse_log_format_value(&value));
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--log-format=") {
            log_format = Some(parse_log_format_value(value));
        }
    }

    if output.is_none() {
        if let Ok(value) = std::env::var("DESLICER_OUTPUT") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                output = Some(parse_log_format_value(trimmed));
            }
        }
    }

    resolve_output_format(output, json_flag, log_format, io::stdout().is_terminal())
}

/// Back-compat alias used by older call sites / tests.
pub fn log_format_from_args<I, S>(args: I) -> LogFormat
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    output_format_from_args(args).unwrap_or(LogFormat::Human)
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
        CliError::InvalidInput(_) | CliError::MissingArgument(_) | CliError::InputRequired(_) => {
            "usage"
        }
        CliError::RateLimited { .. } => "rate_limit",
        CliError::BackendUnavailable(_) => "transport",
        CliError::Transport(_) => "transport",
        CliError::PlanNotFound(_) => "plan",
        CliError::EndpointNotAvailable(_) => "endpoint",
        CliError::HumanApprovalRequired(_) => "approval",
        CliError::AgentRunFailed(_) => "agent",
        CliError::DeviceSessionRequired(_) | CliError::AuthPending(_) => "auth",
        CliError::ValidationModelUnavailable
        | CliError::ValidationTimeout
        | CliError::ValidationUnavailable => "validation",
        CliError::Other(msg) if msg.contains("not logged in") || msg.contains("OIDC") => "auth",
        CliError::Other(_) => "error",
    }
}

pub fn cli_error_code(err: &CliError) -> &'static str {
    match err {
        CliError::OidcRejected(_) => "auth.oidc_rejected",
        CliError::ApiTokenRejected(_) => "auth.api_token_rejected",
        CliError::RepoNotAllowlisted(_) => "auth.repo_not_allowlisted",
        CliError::EnvironmentNotBound(_) => "auth.environment_not_bound",
        CliError::AmbiguousBinding(_) => "auth.ambiguous_binding",
        CliError::GitBackedChangeManagement(_) => "auth.git_backed_change_management",
        CliError::UnsupportedPlatform(_) => "platform.unsupported",
        CliError::InvalidInput(_) => "usage.invalid_input",
        CliError::MissingArgument(_) => "usage.missing_argument",
        CliError::InputRequired(_) => "input.required",
        CliError::AuthPending(_) => "auth.pending",
        CliError::RateLimited { .. } => "transport.rate_limited",
        CliError::BackendUnavailable(_) => "transport.backend_unavailable",
        CliError::Transport(_) => "transport.error",
        CliError::PlanNotFound(_) => "plan.not_found",
        CliError::EndpointNotAvailable(_) => "endpoint.not_available",
        CliError::HumanApprovalRequired(_) => "approval.required",
        CliError::AgentRunFailed(_) => "agent.run_failed",
        CliError::DeviceSessionRequired(_) => "auth.device_session_required",
        CliError::ValidationModelUnavailable => "validation.model_unavailable",
        CliError::ValidationTimeout => "validation.timeout",
        CliError::ValidationUnavailable => "validation.unavailable",
        CliError::Other(msg) if msg.contains("not logged in") => "auth.not_logged_in",
        CliError::Other(msg) if msg.contains("OIDC") => "auth.oidc",
        CliError::Other(_) => "error.other",
    }
}

pub fn cli_error_next_command(err: &CliError) -> Option<&'static str> {
    match err {
        // Already inside auth login / non-interactive — pointing back at login is useless.
        CliError::Other(msg) if msg.contains("without a TTY") => None,
        CliError::AuthPending(_) => Some("deslicer auth login --wait"),
        CliError::InputRequired(msg) if msg.contains("agent") || msg.contains("prompt") => {
            Some("deslicer agent run \"…\"")
        }
        CliError::InputRequired(_) => Some("deslicer --no-input <command> …"),
        CliError::Other(msg) if msg.contains("not logged in") => Some("deslicer auth login"),
        CliError::DeviceSessionRequired(_) => Some("deslicer auth login"),
        CliError::ApiTokenRejected(_) => Some("deslicer auth login"),
        // Confirm which Observer the CLI is talking to (issue #248).
        CliError::EndpointNotAvailable(_) => Some("deslicer auth status"),
        // Recover by listing known plans (issue #260).
        CliError::PlanNotFound(_) => Some("deslicer change list"),
        // Plan id is not always known on the wire error; leave null (no placeholders).
        CliError::HumanApprovalRequired(_) => None,
        _ => None,
    }
}

pub fn cli_error_hint(err: &CliError) -> Option<&'static str> {
    match err {
        CliError::Other(msg) if msg.contains("without a TTY") => {
            Some("set OBSERVER_API_URL and DESLICER_API_TOKEN, or run auth login from a TTY")
        }
        CliError::Other(msg) if msg.contains("not logged in") => {
            Some("run auth login and approve the code in the portal")
        }
        CliError::AuthPending(_) => {
            Some("approve the device code in the portal, or pass --wait to poll")
        }
        CliError::InputRequired(_) => {
            Some("pass required flags/args, or unset --no-input / DESLICER_NO_INPUT for a TTY")
        }
        CliError::DeviceSessionRequired(_) => {
            Some("run deslicer auth login and approve the code in the portal")
        }
        CliError::ApiTokenRejected(_) => {
            Some("mint a tools-scope DESLICER_API_TOKEN in the portal, or run deslicer auth login")
        }
        CliError::HumanApprovalRequired(_) => Some(
            "approve in the portal, or gate CI with a GitHub Environment that requires reviewers",
        ),
        CliError::MissingArgument(msg) if msg.contains("--provider") => {
            Some("pass --provider github|github-token|gitlab|bitbucket|azure")
        }
        CliError::MissingArgument(msg) if msg.contains("--environment") => {
            Some("pass --environment <tenant-slug>")
        }
        CliError::MissingArgument(msg) if msg.contains("--target-group") => {
            Some("pass --target-group <host-group-uuid-or-name>")
        }
        CliError::MissingArgument(msg) if msg.contains("prompt") => {
            Some("pass the prompt as an argument, e.g. deslicer agent run \"…\"")
        }
        CliError::MissingArgument(_) => Some("see the error message for the required flag"),
        // Path detail lives in the error message; hint steers upgrade / URL check (issue #248).
        CliError::EndpointNotAvailable(_) => Some(
            "this Observer does not expose the requested API route; upgrade the backend or check --observer-api-url",
        ),
        CliError::PlanNotFound(_) => Some("check the plan id or list plans"),
        _ => None,
    }
}

fn error_envelope(
    code: &str,
    kind: &str,
    message: String,
    hint: Option<&str>,
    next_command: Option<&str>,
    retryable: bool,
    exit_code: i32,
) -> Value {
    json!({
        "schema_version": 1,
        "error": {
            "code": code,
            "kind": kind,
            "message": message,
            "hint": hint,
            "next_command": next_command,
            "retryable": retryable,
            "exit_code": exit_code,
            "details": {},
        }
    })
}

pub fn cli_error_json(err: &CliError) -> Value {
    error_envelope(
        cli_error_code(err),
        cli_error_kind(err),
        redact_secrets(&err.to_string()),
        cli_error_hint(err),
        cli_error_next_command(err),
        err.retryable(),
        err.exit_code(),
    )
}

pub fn oidc_error_json(err: &OidcError) -> Value {
    let exit_code = oidc_exit_code(err);
    let (code, kind, retryable) = match err {
        OidcError::MissingEnv(_) => ("auth.oidc_missing_env", "oidc", false),
        OidcError::Unsupported(_) => ("platform.unsupported", "oidc", false),
        OidcError::Http(_) => ("transport.oidc_http", "oidc", true),
        OidcError::Other(_) => ("error.oidc", "oidc", false),
    };
    error_envelope(
        code,
        kind,
        redact_secrets(&err.to_string()),
        None,
        None,
        retryable,
        exit_code,
    )
}

pub fn oidc_exit_code(err: &OidcError) -> i32 {
    match err {
        OidcError::MissingEnv(_) => 4,
        OidcError::Unsupported(_) => 8,
        OidcError::Http(_) => 10,
        OidcError::Other(_) => 1,
    }
}

pub fn emit_output_conflict(format: LogFormat) -> i32 {
    let err = CliError::InvalidInput(
        "conflicting values among --output/-o, --json, --log-format, and DESLICER_OUTPUT".into(),
    );
    emit_cli_error(format, &err)
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

/// Exit code for clap parse/help errors.
///
/// Explicit `--help` / `--version` stay 0. A bare group with no subcommand
/// (or a missing required argument) is usage failure → exit 2, even when clap
/// prints help via `DisplayHelpOnMissingArgumentOrSubcommand`.
pub fn clap_error_exit_code(err: &clap::Error) -> i32 {
    match err.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => 0,
        _ => 2,
    }
}

pub fn emit_clap_error(format: LogFormat, err: &clap::Error) -> i32 {
    let code = clap_error_exit_code(err);
    if code == 0 {
        // Help/version stay human on stdout even when the default format is JSON.
        if let Err(print_err) = ignore_broken_pipe(err.print()) {
            write_stderr_line(&format!("clap error print failed: {print_err}"));
        }
        return code;
    }
    match format {
        LogFormat::Json => {
            let message = redact_secrets(&err.to_string());
            write_stderr_line(
                &serde_json::to_string(&error_envelope(
                    "usage.invalid",
                    "usage",
                    message,
                    None,
                    None,
                    false,
                    2,
                ))
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

pub fn emit_log_format_deprecation() {
    write_stderr_line(
        "warning: --log-format is deprecated; prefer --output/-o (or DESLICER_OUTPUT)",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_prefers_explicit_over_tty_default() {
        assert_eq!(
            resolve_output_format(Some(LogFormat::Human), false, None, false).unwrap(),
            LogFormat::Human
        );
        assert_eq!(
            resolve_output_format(None, true, None, true).unwrap(),
            LogFormat::Json
        );
    }

    #[test]
    fn resolve_defaults_json_when_not_tty() {
        assert_eq!(
            resolve_output_format(None, false, None, false).unwrap(),
            LogFormat::Json
        );
        assert_eq!(
            resolve_output_format(None, false, None, true).unwrap(),
            LogFormat::Human
        );
    }

    #[test]
    fn resolve_rejects_conflicts() {
        assert!(resolve_output_format(Some(LogFormat::Human), true, None, false).is_err());
        assert!(
            resolve_output_format(Some(LogFormat::Json), false, Some(LogFormat::Human), true)
                .is_err()
        );
    }

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
    fn output_format_from_args_reads_output_flag() {
        let args = ["cli", "--output", "json", "groups", "list"];
        assert_eq!(output_format_from_args(args).unwrap(), LogFormat::Json);
    }

    #[test]
    fn output_format_from_args_reads_short_o_forms() {
        assert_eq!(
            output_format_from_args(["cli", "-o", "json", "groups", "list"]).unwrap(),
            LogFormat::Json
        );
        assert_eq!(
            output_format_from_args(["cli", "-ohuman", "groups", "list"]).unwrap(),
            LogFormat::Human
        );
    }

    #[test]
    fn output_format_from_args_ignores_ops_like_positionals() {
        // `-ops` must not be swallowed as `-o` + `ps`.
        let format = output_format_from_args(["cli", "-ops", "groups", "list"]).unwrap();
        assert_eq!(
            format,
            resolve_output_format(None, false, None, io::stdout().is_terminal()).unwrap()
        );
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
    fn cli_error_json_has_envelope_v1_fields() {
        let err = CliError::Other("not logged in".into());
        let value = cli_error_json(&err);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["error"]["code"], "auth.not_logged_in");
        assert_eq!(value["error"]["kind"], "auth");
        assert!(value["error"]["hint"].is_string());
        assert_eq!(value["error"]["next_command"], "deslicer auth login");
        assert_eq!(value["error"]["retryable"], false);
        assert_eq!(value["error"]["exit_code"], err.exit_code());
        assert!(value["error"]["details"].is_object());
    }

    #[test]
    fn api_token_rejected_json_is_not_oidc() {
        let err = CliError::ApiTokenRejected("HTTP 401 Unauthorized".into());
        let value = cli_error_json(&err);
        assert_eq!(value["error"]["kind"], "api_token");
        assert_eq!(value["error"]["code"], "auth.api_token_rejected");
        assert!(value["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("DESLICER_API_TOKEN"));
        assert!(!value["error"]["message"].as_str().unwrap().contains("OIDC"));
    }

    #[test]
    fn endpoint_not_available_json_has_hint_and_next_command() {
        let err = CliError::EndpointNotAvailable("path api/v1/catalog → HTTP 404".into());
        let value = cli_error_json(&err);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["error"]["code"], "endpoint.not_available");
        assert_eq!(value["error"]["kind"], "endpoint");
        assert_eq!(value["error"]["exit_code"], 15);
        assert_eq!(value["error"]["next_command"], "deslicer auth status");
        let hint = value["error"]["hint"].as_str().unwrap_or("");
        assert!(
            hint.contains("upgrade the backend") && hint.contains("--observer-api-url"),
            "hint should steer upgrade / URL check, got {hint:?}"
        );
        assert_eq!(value["error"]["retryable"], false);
    }

    #[test]
    fn plan_not_found_json_has_hint_and_next_command() {
        let err = CliError::PlanNotFound("plan xyz not found".into());
        let value = cli_error_json(&err);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["error"]["code"], "plan.not_found");
        assert_eq!(value["error"]["kind"], "plan");
        assert_eq!(value["error"]["exit_code"], 11);
        assert_eq!(value["error"]["next_command"], "deslicer change list");
        assert_eq!(value["error"]["hint"], "check the plan id or list plans");
        assert_eq!(value["error"]["retryable"], false);
    }

    #[test]
    fn device_session_required_json_envelope() {
        let err = CliError::DeviceSessionRequired(
            "no active device session. Run `deslicer auth login` first".into(),
        );
        let value = cli_error_json(&err);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["error"]["code"], "auth.device_session_required");
        assert_eq!(value["error"]["kind"], "auth");
        assert_eq!(value["error"]["exit_code"], 4);
        assert_eq!(value["error"]["next_command"], "deslicer auth login");
        assert!(value["error"]["hint"].is_string());
        assert_eq!(value["error"]["retryable"], false);
    }

    #[test]
    fn ignore_broken_pipe_swallows_epipe_only() {
        assert!(ignore_broken_pipe(Ok(())).is_ok());
        assert!(ignore_broken_pipe(Err(io::Error::from(io::ErrorKind::BrokenPipe))).is_ok());
        let err = ignore_broken_pipe(Err(io::Error::from(io::ErrorKind::PermissionDenied)));
        assert_eq!(err.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn bare_groups_are_usage_exit_2() {
        use clap::Parser;

        use crate::cli::Cli;

        for argv in [
            &["deslicer", "change"][..],
            &["deslicer", "acs", "stacks"][..],
            &["deslicer", "models", "byok"][..],
            &["deslicer", "org", "settings"][..],
            &["deslicer"][..],
        ] {
            let Err(err) = Cli::try_parse_from(argv) else {
                panic!("expected clap error for {argv:?}");
            };
            assert_eq!(
                err.kind(),
                ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand,
                "{argv:?}"
            );
            assert_eq!(clap_error_exit_code(&err), 2, "{argv:?}");
        }
    }

    #[test]
    fn explicit_help_and_version_stay_exit_0() {
        use clap::Parser;

        use crate::cli::Cli;

        let Err(help) = Cli::try_parse_from(["deslicer", "change", "--help"]) else {
            panic!("expected help error");
        };
        assert_eq!(help.kind(), ErrorKind::DisplayHelp);
        assert_eq!(clap_error_exit_code(&help), 0);

        let Err(version) = Cli::try_parse_from(["deslicer", "--version"]) else {
            panic!("expected version error");
        };
        assert_eq!(version.kind(), ErrorKind::DisplayVersion);
        assert_eq!(clap_error_exit_code(&version), 0);
    }
}
