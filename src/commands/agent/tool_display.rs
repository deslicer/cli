//! Human labels and visibility for agent tool names.

use serde_json::Value;

/// How a tool should appear on the progress stream.
pub struct ToolDisplay {
    raw: String,
}

impl ToolDisplay {
    pub fn new(raw: impl Into<String>) -> Self {
        Self { raw: raw.into() }
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Orchestrator bookkeeping that is noise unless `--verbose`.
    pub fn is_internal(&self) -> bool {
        matches!(
            self.raw.as_str(),
            "declare_intent"
                | "createTaskList"
                | "create_task_list"
                | "updateTaskProgress"
                | "update_task_progress"
        )
    }

    pub fn label(&self) -> String {
        match self.raw.as_str() {
            "declare_intent" => "Setting intent".into(),
            "createTaskList" | "create_task_list" => "Planning steps".into(),
            "updateTaskProgress" | "update_task_progress" => "Updating progress".into(),
            "search_tool" => "Searching tools".into(),
            "run_tool" => "Running a tool".into(),
            other => humanize(other),
        }
    }

    /// Short hint from a `tool-input-available` payload, if one is obvious.
    pub fn input_detail(input: Option<&Value>) -> Option<String> {
        let input = input?;
        const KEYS: &[&str] = &[
            "tool", "toolName", "name", "query", "q", "title", "path", "command",
        ];
        if let Some(obj) = input.as_object() {
            for key in KEYS {
                if let Some(text) = obj.get(*key).and_then(Value::as_str) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(truncate_detail(trimmed));
                    }
                }
            }
        }
        input.as_str().map(str::trim).and_then(|text| {
            if text.is_empty() {
                None
            } else {
                Some(truncate_detail(text))
            }
        })
    }
}

fn humanize(name: &str) -> String {
    let mut words = Vec::new();
    let mut current = String::new();
    for c in name.chars() {
        if c == '_' || c == '-' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        if c.is_uppercase() && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    if words.is_empty() {
        return name.to_string();
    }
    let joined = words.join(" ");
    let mut chars = joined.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => name.to_string(),
    }
}

fn truncate_detail(text: &str) -> String {
    const LIMIT: usize = 72;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let mut out: String = text.chars().take(LIMIT.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn known_tools_have_stable_labels() {
        assert_eq!(ToolDisplay::new("search_tool").label(), "Searching tools");
        assert_eq!(ToolDisplay::new("run_tool").label(), "Running a tool");
        assert_eq!(ToolDisplay::new("createTaskList").label(), "Planning steps");
    }

    #[test]
    fn orchestrator_bookkeeping_is_internal() {
        assert!(ToolDisplay::new("declare_intent").is_internal());
        assert!(ToolDisplay::new("updateTaskProgress").is_internal());
        assert!(!ToolDisplay::new("search_tool").is_internal());
        assert!(!ToolDisplay::new("search_hosts").is_internal());
    }

    #[test]
    fn snake_and_camel_names_become_sentence_case() {
        assert_eq!(ToolDisplay::new("search_hosts").label(), "Search hosts");
        assert_eq!(ToolDisplay::new("listPlans").label(), "List plans");
    }

    #[test]
    fn input_detail_prefers_a_tool_name() {
        let input = json!({"tool": "list_splunk_apps", "limit": 50});
        assert_eq!(
            ToolDisplay::input_detail(Some(&input)).as_deref(),
            Some("list_splunk_apps")
        );
    }

    #[test]
    fn input_detail_truncates_long_queries() {
        let query = "a".repeat(80);
        let input = json!({"query": query});
        let detail = ToolDisplay::input_detail(Some(&input)).expect("detail");
        assert!(detail.ends_with('…'));
        assert!(detail.chars().count() <= 72);
    }
}
