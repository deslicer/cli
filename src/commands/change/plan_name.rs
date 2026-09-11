//! Resolve a human-readable plan `name` for `deslicer change plan`.
//!
//! Explicit `--name` wins. Otherwise, in GitHub Actions, prefer the pull
//! request title, then the push head-commit subject, then `git log -1`.
//! Observer stores the value on `change_plans.name` (max 255).

use std::process::Command;

use serde_json::Value;

/// Observer `CreateChangePlanRequest.name` validation cap.
const PLAN_NAME_MAX_LEN: usize = 255;

/// Resolve the plan name to send on create.
///
/// Returns `None` only when nothing usable is available (local runs without
/// `--name` and without a readable git HEAD subject).
pub fn resolve_plan_name(explicit: Option<&str>) -> Option<String> {
    if let Some(name) = normalize_plan_name(explicit) {
        return Some(name);
    }
    if let Some(name) = github_actions_plan_name() {
        return Some(name);
    }
    normalize_plan_name(git_head_subject().as_deref())
}

fn normalize_plan_name(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let subject = first_line(trimmed);
    if subject.is_empty() {
        return None;
    }
    Some(truncate_plan_name(subject))
}

fn first_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
}

fn truncate_plan_name(name: &str) -> String {
    if name.chars().count() <= PLAN_NAME_MAX_LEN {
        return name.to_string();
    }
    name.chars().take(PLAN_NAME_MAX_LEN).collect()
}

fn github_actions_plan_name() -> Option<String> {
    if std::env::var_os("GITHUB_ACTIONS").is_none() {
        return None;
    }
    let event_path = std::env::var_os("GITHUB_EVENT_PATH")?;
    let body: Value = serde_json::from_str(&std::fs::read_to_string(event_path).ok()?).ok()?;
    let event_name = std::env::var("GITHUB_EVENT_NAME").unwrap_or_default();

    if matches!(event_name.as_str(), "pull_request" | "pull_request_target") {
        if let Some(title) = body
            .pointer("/pull_request/title")
            .and_then(Value::as_str)
            .and_then(|s| normalize_plan_name(Some(s)))
        {
            return Some(title);
        }
    }

    if let Some(title) = body
        .pointer("/head_commit/message")
        .and_then(Value::as_str)
        .and_then(|s| normalize_plan_name(Some(s)))
    {
        return Some(title);
    }

    None
}

fn git_head_subject() -> Option<String> {
    let output = Command::new("git")
        .args(["log", "-1", "--pretty=%s"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    normalize_plan_name(Some(text.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::{first_line, normalize_plan_name, truncate_plan_name, PLAN_NAME_MAX_LEN};

    #[test]
    fn explicit_name_is_trimmed() {
        assert_eq!(
            normalize_plan_name(Some("  Harden TLS  ")).as_deref(),
            Some("Harden TLS")
        );
    }

    #[test]
    fn blank_explicit_name_is_ignored() {
        assert_eq!(normalize_plan_name(Some("   ")), None);
        assert_eq!(normalize_plan_name(Some("")), None);
        assert_eq!(normalize_plan_name(None), None);
    }

    #[test]
    fn multiline_commit_uses_subject_only() {
        assert_eq!(
            normalize_plan_name(Some("feat: add inputs\n\nLonger body")).as_deref(),
            Some("feat: add inputs")
        );
    }

    #[test]
    fn truncates_to_observer_limit() {
        let long: String = "a".repeat(PLAN_NAME_MAX_LEN + 40);
        let truncated = truncate_plan_name(&long);
        assert_eq!(truncated.chars().count(), PLAN_NAME_MAX_LEN);
    }

    #[test]
    fn first_line_skips_leading_blank_lines() {
        assert_eq!(first_line("\n\nSubject\nbody"), "Subject");
    }
}
