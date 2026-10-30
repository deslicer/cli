//! Idempotency-Key defaults for plan-creating commands (#228).
//!
//! Spec lock: default a key **only** when a CI run id is present or the caller
//! set `--idempotency-key`. Never hash local inputs alone for silent laptop
//! replay. When CI is present, the default key includes a hash of the plan
//! inputs (plus attempt/job when available) so two plan steps in one run do
//! not share a key.

use sha2::{Digest, Sha256};
use std::cell::Cell;

thread_local! {
    static LAST_REPLAYED: Cell<bool> = const { Cell::new(false) };
}

/// Join distinguishing CLI / request inputs into a stable fingerprint string.
pub fn fingerprint(parts: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let mut out = String::new();
    for (i, part) in parts.into_iter().enumerate() {
        if i > 0 {
            out.push('\u{1f}');
        }
        out.push_str(part.as_ref().trim());
    }
    out
}

/// Resolve the Idempotency-Key to send, if any.
///
/// `input_fingerprint` is hashed into the default CI key so distinct plan
/// creates in the same run get distinct keys. It is ignored when an explicit
/// key is set. Without a CI run id there is still no default key.
pub fn resolve(explicit: Option<&str>, input_fingerprint: &str) -> Option<String> {
    if let Some(raw) = explicit {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let run_id = ci_run_id()?;
    let mut key = format!("deslicer-ci-{run_id}");
    if let Some(attempt) = ci_run_attempt() {
        key.push_str("-a");
        key.push_str(&attempt);
    }
    if let Some(job) = ci_job_id() {
        key.push_str("-j");
        key.push_str(&sanitize_key_segment(&job));
    }
    key.push('-');
    key.push_str(&short_input_hash(input_fingerprint));
    Some(key)
}

fn short_input_hash(inputs: &str) -> String {
    let digest = Sha256::digest(inputs.as_bytes());
    hex::encode(&digest[..6])
}

fn sanitize_key_segment(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn ci_run_id() -> Option<String> {
    const KEYS: &[&str] = &[
        "GITHUB_RUN_ID",
        "CI_PIPELINE_ID",
        "BUILD_BUILDID",
        "BITBUCKET_BUILD_NUMBER",
        "CIRCLE_BUILD_NUM",
    ];
    first_nonempty_env(KEYS)
}

fn ci_run_attempt() -> Option<String> {
    first_nonempty_env(&["GITHUB_RUN_ATTEMPT"])
}

fn ci_job_id() -> Option<String> {
    first_nonempty_env(&[
        "GITHUB_JOB",
        "CI_JOB_ID",
        "SYSTEM_JOBID",
        "BITBUCKET_STEP_UUID",
        "CIRCLE_JOB",
    ])
}

fn first_nonempty_env(keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// True when this Observer path is a plan-create endpoint that may receive
/// `Idempotency-Key`. Follow-up mutating calls (bundle upload, compile trigger,
/// ACS project-plan attach, …) must not reuse the create key.
pub fn is_plan_create_path(path: &str) -> bool {
    let path = path.trim_start_matches('/');
    let path = path.split('?').next().unwrap_or(path);
    path == "v1/plan"
        || path == "api/v1/plans"
        || path == "api/v1/remediate/draft"
        || path == "api/v1/splunk-packages/upgrade-rolling"
        || path.strip_prefix("api/v1/catalog/").is_some_and(|rest| {
            matches!(rest.rsplit_once('/'), Some((id, "plan")) if !id.is_empty())
        })
}

pub fn note_replayed(replayed: bool) {
    LAST_REPLAYED.with(|cell| cell.set(replayed));
}

pub fn take_replayed() -> bool {
    LAST_REPLAYED.with(|cell| cell.replace(false))
}

/// True when Observer marks the response as an idempotent replay.
pub fn response_indicates_replay(headers: &reqwest::header::HeaderMap, body: &[u8]) -> bool {
    if headers
        .get("idempotency-replayed")
        .or_else(|| headers.get("idempotent-replayed"))
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            let t = v.trim();
            t.eq_ignore_ascii_case("true") || t == "1"
        })
    {
        return true;
    }
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) {
        if value.get("replayed").and_then(|v| v.as_bool()) == Some(true) {
            return true;
        }
        if value.pointer("/plan/replayed").and_then(|v| v.as_bool()) == Some(true) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_ci_env() {
        for name in [
            "GITHUB_RUN_ID",
            "CI_PIPELINE_ID",
            "BUILD_BUILDID",
            "BITBUCKET_BUILD_NUMBER",
            "CIRCLE_BUILD_NUM",
            "GITHUB_RUN_ATTEMPT",
            "GITHUB_JOB",
            "CI_JOB_ID",
            "SYSTEM_JOBID",
            "BITBUCKET_STEP_UUID",
            "CIRCLE_JOB",
        ] {
            std::env::remove_var(name);
        }
    }

    #[test]
    fn explicit_key_wins() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_ci_env();
        assert_eq!(
            resolve(Some("  my-key  "), "ignored").as_deref(),
            Some("my-key")
        );
    }

    #[test]
    fn ci_run_id_defaults_key_with_input_hash() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_ci_env();
        std::env::set_var("GITHUB_RUN_ID", "12345");
        let a = resolve(None, "change plan\u{1f}staging").unwrap();
        let b = resolve(None, "change plan\u{1f}prod").unwrap();
        assert!(a.starts_with("deslicer-ci-12345-"));
        assert_ne!(a, b, "distinct inputs must yield distinct CI keys");
        assert_eq!(a, resolve(None, "change plan\u{1f}staging").unwrap());
        std::env::remove_var("GITHUB_RUN_ID");
        assert_eq!(resolve(None, "anything"), None);
    }

    #[test]
    fn ci_attempt_and_job_are_included() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_ci_env();
        std::env::set_var("GITHUB_RUN_ID", "99");
        std::env::set_var("GITHUB_RUN_ATTEMPT", "2");
        std::env::set_var("GITHUB_JOB", "plan-prod");
        let key = resolve(None, "fp").unwrap();
        let hash = short_input_hash("fp");
        assert_eq!(key, format!("deslicer-ci-99-a2-jplan-prod-{hash}"));
        clear_ci_env();
    }

    #[test]
    fn empty_explicit_falls_through_to_ci() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_ci_env();
        std::env::set_var("CI_PIPELINE_ID", "9");
        let key = resolve(Some("  "), "x").unwrap();
        assert!(key.starts_with("deslicer-ci-9-"));
        assert!(key.ends_with(&short_input_hash("x")));
        std::env::remove_var("CI_PIPELINE_ID");
    }

    #[test]
    fn take_replayed_clears_latch() {
        note_replayed(true);
        assert!(take_replayed());
        assert!(!take_replayed());
    }

    #[test]
    fn detects_replayed_header_and_body() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Idempotency-Replayed", "true".parse().unwrap());
        assert!(response_indicates_replay(&headers, b"{}"));
        assert!(response_indicates_replay(
            &reqwest::header::HeaderMap::new(),
            br#"{"replayed":true,"plan":{"id":"x"}}"#
        ));
        assert!(!response_indicates_replay(
            &reqwest::header::HeaderMap::new(),
            br#"{"plan":{"id":"x"}}"#
        ));
    }

    #[test]
    fn plan_create_paths_only() {
        assert!(is_plan_create_path("api/v1/plans"));
        assert!(is_plan_create_path("/api/v1/plans?environment=prod"));
        assert!(is_plan_create_path("v1/plan"));
        assert!(is_plan_create_path("api/v1/remediate/draft"));
        assert!(is_plan_create_path("api/v1/catalog/bp-1/plan"));
        assert!(is_plan_create_path(
            "api/v1/splunk-packages/upgrade-rolling"
        ));
        assert!(!is_plan_create_path("api/v1/plan-sources/bundles"));
        assert!(!is_plan_create_path("api/v1/runners/compile/row-1"));
        assert!(!is_plan_create_path(
            "api/v1/acs/stacks/prod/project-plan"
        ));
        assert!(!is_plan_create_path("api/v1/plans/abc/approve"));
        assert!(!is_plan_create_path("api/v1/remediate/analyse"));
        assert!(!is_plan_create_path(
            "api/v1/splunk-packages/upgrade-rolling/schedule"
        ));
        assert!(!is_plan_create_path("api/v1/catalog/bp-1/plan/extra"));
    }
}
