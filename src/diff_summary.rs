//! Parse dry-run diff payloads from Observer `GET /api/v1/plans/{id}/diff`.

use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffCounts {
    pub total: u64,
    pub additions: u64,
    pub modifications: u64,
    pub deletions: u64,
    pub has_destructive: bool,
}

impl DiffCounts {
    pub fn human_summary(&self) -> String {
        let destructive = if self.has_destructive {
            " (includes deletions)"
        } else {
            ""
        };
        format!(
            "{} change(s): +{} ~{} -{}{}",
            self.total, self.additions, self.modifications, self.deletions, destructive
        )
    }
}

/// Extract change counts from a `PlanDiffResponse` or bare dry-run JSON body.
pub fn diff_counts_from_observer_value(root: &Value) -> Option<DiffCounts> {
    let summary = summary_object(root)?;
    Some(DiffCounts {
        total: json_u64(summary, "total"),
        additions: json_u64(summary, "additions"),
        modifications: json_u64(summary, "modifications"),
        deletions: json_u64(summary, "deletions"),
        has_destructive: summary
            .get("has_destructive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn summary_object(root: &Value) -> Option<&Value> {
    root.get("diff")
        .and_then(summary_in_node)
        .or_else(|| summary_in_node(root))
}

fn summary_in_node(node: &Value) -> Option<&Value> {
    node.get("summary").or_else(|| {
        node.get("diff_json")
            .and_then(|inner| inner.get("summary"))
    })
}

fn json_u64(obj: &Value, key: &str) -> u64 {
    obj.get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_plan_diff_response_shape() {
        let body = json!({
            "plan_id": "019f439b-8493-7f7d-b1ef-f082a483cef8",
            "diff": {
                "summary": {
                    "total": 3,
                    "additions": 1,
                    "modifications": 1,
                    "deletions": 1,
                    "has_destructive": true
                }
            }
        });
        let counts = diff_counts_from_observer_value(&body).expect("counts");
        assert_eq!(counts.total, 3);
        assert_eq!(counts.additions, 1);
        assert!(counts.has_destructive);
        assert!(counts.human_summary().contains("3 change(s)"));
    }

    #[test]
    fn parses_legacy_diff_json_nesting() {
        let body = json!({
            "diff_json": {
                "summary": { "total": 2, "additions": 2, "modifications": 0, "deletions": 0 }
            }
        });
        let counts = diff_counts_from_observer_value(&body).expect("counts");
        assert_eq!(counts.total, 2);
    }
}
