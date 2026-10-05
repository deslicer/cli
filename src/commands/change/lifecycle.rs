//! Plan lifecycle status values from Observer `ChangePlan.status`.

pub fn is_still_compiling(status: &str) -> bool {
    matches!(status, "draft" | "compiling" | "compile_pending")
}

pub fn is_compile_failure(status: &str) -> bool {
    matches!(status, "failed" | "compile_failed" | "rejected")
}

/// Lifecycle states where execution progress will not advance further.
pub fn is_terminal_lifecycle(status: &str) -> bool {
    matches!(
        status,
        "failed" | "compile_failed" | "expired" | "rejected" | "completed" | "no_changes"
    )
}

/// Non-zero exit for terminal failure lifecycle states (`change status`, CI branching).
pub fn plan_status_exit_code(status: &str) -> i32 {
    if matches!(status, "failed" | "compile_failed" | "rejected" | "expired") {
        14
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_lifecycle_includes_issue_122_states() {
        for status in [
            "failed",
            "compile_failed",
            "expired",
            "rejected",
            "completed",
            "no_changes",
        ] {
            assert!(is_terminal_lifecycle(status), "{status} should be terminal");
        }
        assert!(!is_terminal_lifecycle("pending_approval"));
        assert!(!is_terminal_lifecycle("executing"));
    }

    #[test]
    fn failure_exit_code_only_for_failure_states() {
        assert_eq!(plan_status_exit_code("failed"), 14);
        assert_eq!(plan_status_exit_code("compile_failed"), 14);
        assert_eq!(plan_status_exit_code("rejected"), 14);
        assert_eq!(plan_status_exit_code("expired"), 14);
        assert_eq!(plan_status_exit_code("completed"), 0);
        assert_eq!(plan_status_exit_code("no_changes"), 0);
        assert_eq!(plan_status_exit_code("pending_approval"), 0);
    }
}
