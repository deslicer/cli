//! Idempotency-Key defaults for plan-creating commands (#228).
//!
//! Spec lock: default a key **only** when a CI run id is present or the caller
//! set `--idempotency-key`. Never hash local inputs for silent laptop replay.

use std::cell::Cell;

thread_local! {
    static LAST_REPLAYED: Cell<bool> = const { Cell::new(false) };
}

/// Resolve the Idempotency-Key to send, if any.
pub fn resolve(explicit: Option<&str>) -> Option<String> {
    if let Some(raw) = explicit {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    ci_run_id().map(|id| format!("deslicer-ci-{id}"))
}

fn ci_run_id() -> Option<String> {
    const KEYS: &[&str] = &[
        "GITHUB_RUN_ID",
        "CI_PIPELINE_ID",
        "BUILD_BUILDID",
        "BITBUCKET_BUILD_NUMBER",
        "CIRCLE_BUILD_NUM",
    ];
    for key in KEYS {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
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

    #[test]
    fn explicit_key_wins() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("GITHUB_RUN_ID");
        assert_eq!(resolve(Some("  my-key  ")).as_deref(), Some("my-key"));
    }

    #[test]
    fn ci_run_id_defaults_key() {
        let _guard = ENV_LOCK.lock().unwrap();
        for name in [
            "CI_PIPELINE_ID",
            "BUILD_BUILDID",
            "BITBUCKET_BUILD_NUMBER",
            "CIRCLE_BUILD_NUM",
        ] {
            std::env::remove_var(name);
        }
        std::env::set_var("GITHUB_RUN_ID", "12345");
        assert_eq!(resolve(None).as_deref(), Some("deslicer-ci-12345"));
        std::env::remove_var("GITHUB_RUN_ID");
        assert_eq!(resolve(None), None);
    }

    #[test]
    fn empty_explicit_falls_through_to_ci() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("GITHUB_RUN_ID");
        std::env::set_var("CI_PIPELINE_ID", "9");
        assert_eq!(resolve(Some("  ")).as_deref(), Some("deslicer-ci-9"));
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
}
