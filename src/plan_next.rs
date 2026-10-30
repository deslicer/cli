//! Uniform `plan_id` + `next_command` / `next_commands` for create/advance output (#225).

use serde_json::{json, Value};

use crate::observer_client::ChangePlan;

/// Primary next step and alternatives for a plan handle, keyed by status.
pub fn next_commands_for(plan_id: &str, status: &str) -> (Option<String>, Vec<String>) {
    let status_cmd = format!("deslicer change status --plan-id {plan_id}");
    let approve_cmd = format!("deslicer change approve --plan-id {plan_id}");
    let deploy_cmd = format!("deslicer change deploy --plan-id {plan_id}");
    let retry_cmd = format!("deslicer change retry --plan-id {plan_id} --yes");

    match normalize_status(status) {
        "draft" | "pending" | "pending_approval" | "compiled" | "compile_succeeded" => {
            (Some(status_cmd.clone()), vec![status_cmd, approve_cmd])
        }
        "approved" | "approved_unsigned" => {
            (Some(deploy_cmd.clone()), vec![deploy_cmd, status_cmd])
        }
        "executing" | "running" | "in_progress" | "queued" => {
            (Some(status_cmd.clone()), vec![status_cmd])
        }
        "failed" | "compile_failed" => (Some(retry_cmd.clone()), vec![retry_cmd, status_cmd]),
        "rejected" | "expired" | "cancelled" | "canceled" => (None, vec![]),
        "completed" | "succeeded" | "deployed" => (None, vec![]),
        _ => (Some(status_cmd.clone()), vec![status_cmd]),
    }
}

fn normalize_status(status: &str) -> &str {
    status.trim()
}

/// Enrich a plan success document with the agent-native handle fields.
pub fn attach_plan_handle(mut value: Value, plan_id: &str, status: &str) -> Value {
    let (next, nexts) = next_commands_for(plan_id, status);
    let Some(obj) = value.as_object_mut() else {
        return json!({
            "plan_id": plan_id,
            "status": status,
            "url": Value::Null,
            "next_command": next,
            "next_commands": nexts,
            "value": value,
        });
    };
    obj.insert("plan_id".into(), json!(plan_id));
    obj.insert("status".into(), json!(status));
    obj.entry("url".to_string()).or_insert(Value::Null);
    obj.insert("next_command".into(), json!(next));
    obj.insert("next_commands".into(), json!(nexts));
    value
}

pub fn change_plan_success_doc(plan: &ChangePlan) -> Value {
    let plan_id = plan.external_id().to_string();
    let status = plan.status.clone();
    let raw = serde_json::to_value(plan).unwrap_or_else(|_| json!({}));
    attach_plan_handle(raw, &plan_id, &status)
}

/// One-line human summary of the primary next step (empty when none).
pub fn human_next_line(plan_id: &str, status: &str) -> String {
    match next_commands_for(plan_id, status) {
        (Some(next), _) => format!("next: {next}\n"),
        (None, _) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_approval_suggests_status_then_approve() {
        let (next, nexts) = next_commands_for("p-1", "pending_approval");
        assert_eq!(
            next.as_deref(),
            Some("deslicer change status --plan-id p-1")
        );
        assert_eq!(
            nexts,
            vec![
                "deslicer change status --plan-id p-1".to_string(),
                "deslicer change approve --plan-id p-1".to_string(),
            ]
        );
    }

    #[test]
    fn approved_suggests_deploy() {
        let (next, _) = next_commands_for("p-2", "approved");
        assert_eq!(
            next.as_deref(),
            Some("deslicer change deploy --plan-id p-2")
        );
    }

    #[test]
    fn attach_adds_handle_fields_without_dropping_plan_body() {
        let plan = ChangePlan {
            id: "row".into(),
            plan_id: Some("ext".into()),
            status: "draft".into(),
            name: Some("n".into()),
            summary: None,
        };
        let doc = change_plan_success_doc(&plan);
        assert_eq!(doc["plan_id"], "ext");
        assert_eq!(doc["status"], "draft");
        assert_eq!(doc["name"], "n");
        assert!(doc["url"].is_null());
        assert_eq!(doc["next_command"], "deslicer change status --plan-id ext");
        assert!(doc["next_commands"].as_array().unwrap().len() >= 1);
    }
}
