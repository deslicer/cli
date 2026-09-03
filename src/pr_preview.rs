//! PR preview labeling: which plan apps this PR touches vs leftover drift.
//!
//! Clarity only — does **not** change which apps the plan packs or executes.
//! Full desired-vs-observed reconcile still includes every drifted mapped app.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::environment_yaml::extract_apps_blocks;

/// Apps in a compiled plan, split by whether the PR/changed paths touched them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrPreviewLabels {
    /// Drifted apps whose `source_path` overlaps a changed path.
    pub pr_touched_apps: Vec<String>,
    /// Drifted apps still in the plan that this PR did not touch.
    pub also_still_drifted_apps: Vec<String>,
}

impl PrPreviewLabels {
    pub fn is_empty(&self) -> bool {
        self.pr_touched_apps.is_empty() && self.also_still_drifted_apps.is_empty()
    }

    pub fn human_summary(&self) -> String {
        let touched = if self.pr_touched_apps.is_empty() {
            "(none)".to_string()
        } else {
            self.pr_touched_apps.join(", ")
        };
        let drifted = if self.also_still_drifted_apps.is_empty() {
            "(none)".to_string()
        } else {
            self.also_still_drifted_apps.join(", ")
        };
        format!("this PR touches: {touched}; also still drifted: {drifted}")
    }

    pub fn markdown_section(&self) -> String {
        let mut lines = vec![
            "### PR preview (labeling only)".to_string(),
            String::new(),
            "Full reconcile still packs every drifted mapped app. Labels do not filter execute."
                .to_string(),
            String::new(),
            format!(
                "- **This PR touches:** {}",
                format_app_list(&self.pr_touched_apps)
            ),
            format!(
                "- **Also still drifted:** {}",
                format_app_list(&self.also_still_drifted_apps)
            ),
        ];
        lines.push(String::new());
        lines.join("\n")
    }
}

fn format_app_list(apps: &[String]) -> String {
    if apps.is_empty() {
        "_(none)_".to_string()
    } else {
        apps.iter()
            .map(|app| format!("`{app}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Unique `app_name` values from a plan dry-run / PlanDiffResponse body.
pub fn app_names_from_diff(root: &Value) -> Vec<String> {
    let mut names = BTreeSet::new();
    for item in change_items(root) {
        if let Some(name) = item.get("app_name").and_then(Value::as_str) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                names.insert(trimmed.to_string());
            }
        }
    }
    names.into_iter().collect()
}

fn change_items(root: &Value) -> &[Value] {
    root.get("diff")
        .and_then(|diff| diff.get("change_items"))
        .or_else(|| root.get("change_items"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Mapped apps from environment YAML (`source_path` → display label = basename).
pub fn mapped_apps_from_yaml(env_yaml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for block in extract_apps_blocks(env_yaml) {
        for source_path in block.source_paths() {
            let normalized = normalize_repo_path(&source_path);
            if normalized.is_empty() || !seen.insert(normalized.clone()) {
                continue;
            }
            let label = app_label_from_source_path(&normalized);
            out.push((normalized, label));
        }
    }
    out
}

fn app_label_from_source_path(source_path: &str) -> String {
    Path::new(source_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(source_path)
        .to_string()
}

fn normalize_repo_path(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

/// True when `changed` is the mapped app root or a file under it.
pub fn path_touches_source(changed: &str, source_path: &str) -> bool {
    let changed = normalize_repo_path(changed);
    let source = normalize_repo_path(source_path);
    if changed.is_empty() || source.is_empty() {
        return false;
    }
    changed == source || changed.starts_with(&(source + "/"))
}

/// Labels for apps that appear in the plan diff, given PR/changed paths.
///
/// When `changed_paths` is empty, returns `None` (no labeling context).
/// Mapping YAML changes under `.deslicer/environments/` mark every mapped app
/// as touched for labeling (still does not filter execute).
pub fn label_plan_apps(
    drifted_app_names: &[String],
    mapped_apps: &[(String, String)],
    changed_paths: &[String],
) -> Option<PrPreviewLabels> {
    if changed_paths.is_empty() || drifted_app_names.is_empty() {
        return None;
    }

    let env_mapping_changed = changed_paths.iter().any(|path| {
        let normalized = normalize_repo_path(path);
        normalized.starts_with(".deslicer/environments/") || normalized == ".deslicer/environments"
    });

    let mut touched_labels = BTreeSet::new();
    for (source_path, label) in mapped_apps {
        let hit = env_mapping_changed
            || changed_paths
                .iter()
                .any(|path| path_touches_source(path, source_path));
        if hit {
            touched_labels.insert(label.clone());
        }
    }

    // Also match drifted app_name directly against path basenames / segments when
    // YAML was unavailable (direct UUID plan without local env file).
    if mapped_apps.is_empty() {
        for app in drifted_app_names {
            let needle = format!("/{app}/");
            let suffix = format!("/{app}");
            if changed_paths.iter().any(|path| {
                let normalized = normalize_repo_path(path);
                normalized == *app || normalized.ends_with(&suffix) || normalized.contains(&needle)
            }) {
                touched_labels.insert(app.clone());
            }
        }
    }

    let mut pr_touched = Vec::new();
    let mut also_drifted = Vec::new();
    for app in drifted_app_names {
        if touched_labels.contains(app) {
            pr_touched.push(app.clone());
        } else {
            also_drifted.push(app.clone());
        }
    }

    Some(PrPreviewLabels {
        pr_touched_apps: pr_touched,
        also_still_drifted_apps: also_drifted,
    })
}

/// Resolve changed paths from CLI flags / env / GitHub PR git range.
pub fn resolve_changed_paths(
    changed_paths_file: Option<&Path>,
    changed_paths_csv: Option<&str>,
) -> Option<Vec<String>> {
    if let Some(file) = changed_paths_file {
        return read_paths_file(file);
    }
    if let Some(csv) = changed_paths_csv
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(split_path_list(csv));
    }
    if let Ok(raw) = std::env::var("DESLICER_CHANGED_PATHS") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(split_path_list(trimmed));
        }
    }
    discover_github_pr_changed_paths()
}

fn split_path_list(raw: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = BTreeSet::new();
    for part in raw.split(['\n', '\r', ',', ';']) {
        let path = normalize_repo_path(part);
        if !path.is_empty() && seen.insert(path.clone()) {
            paths.push(path);
        }
    }
    paths
}

fn read_paths_file(path: &Path) -> Option<Vec<String>> {
    let content = std::fs::read_to_string(path).ok()?;
    let paths = split_path_list(&content);
    if paths.is_empty() {
        None
    } else {
        Some(paths)
    }
}

fn discover_github_pr_changed_paths() -> Option<Vec<String>> {
    let event_name = std::env::var("GITHUB_EVENT_NAME").ok()?;
    if event_name != "pull_request" && event_name != "pull_request_target" {
        return None;
    }
    let event_path = std::env::var("GITHUB_EVENT_PATH").ok()?;
    let body: Value = serde_json::from_str(&std::fs::read_to_string(event_path).ok()?).ok()?;
    let base = body
        .pointer("/pull_request/base/sha")
        .and_then(Value::as_str)?
        .to_string();
    let head = body
        .pointer("/pull_request/head/sha")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| std::env::var("GITHUB_SHA").ok())?;
    git_diff_name_only(&base, &head)
}

fn git_diff_name_only(base: &str, head: &str) -> Option<Vec<String>> {
    let output = Command::new("git")
        .args(["diff", "--name-only", &format!("{base}...{head}")])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let paths = split_path_list(&text);
    if paths.is_empty() {
        None
    } else {
        Some(paths)
    }
}

/// Build labels from a dry-run body + optional env YAML + changed paths.
pub fn labels_for_plan_context(
    diff_body: Option<&Value>,
    env_yaml: Option<&str>,
    changed_paths: &[String],
) -> Option<PrPreviewLabels> {
    let diff = diff_body?;
    let drifted = app_names_from_diff(diff);
    if drifted.is_empty() {
        return None;
    }
    let mapped = env_yaml.map(mapped_apps_from_yaml).unwrap_or_default();
    label_plan_apps(&drifted, &mapped, changed_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_unique_app_names_from_plan_diff() {
        let body = json!({
            "diff": {
                "change_items": [
                    { "app_name": "demo_ci_app", "config_path": "local/inputs.conf" },
                    { "app_name": "TA-linux", "config_path": "default/props.conf" },
                    { "app_name": "demo_ci_app", "config_path": "local/props.conf" }
                ]
            }
        });
        assert_eq!(
            app_names_from_diff(&body),
            vec!["TA-linux".to_string(), "demo_ci_app".to_string()]
        );
    }

    #[test]
    fn path_prefix_match_is_rooted() {
        assert!(path_touches_source(
            "apps/demo_ci_app/local/inputs.conf",
            "apps/demo_ci_app"
        ));
        assert!(path_touches_source("apps/demo_ci_app", "apps/demo_ci_app"));
        assert!(!path_touches_source(
            "apps/demo_ci_app_extra/local/x.conf",
            "apps/demo_ci_app"
        ));
    }

    #[test]
    fn labels_split_touched_vs_leftover_drift() {
        let drifted = vec!["demo_ci_app".into(), "TA-linux".into()];
        let mapped = vec![
            ("apps/demo_ci_app".into(), "demo_ci_app".into()),
            ("apps/TA-linux".into(), "TA-linux".into()),
        ];
        let changed = vec!["apps/demo_ci_app/local/inputs.conf".into()];
        let labels = label_plan_apps(&drifted, &mapped, &changed).expect("labels");
        assert_eq!(labels.pr_touched_apps, vec!["demo_ci_app".to_string()]);
        assert_eq!(labels.also_still_drifted_apps, vec!["TA-linux".to_string()]);
        assert!(labels
            .human_summary()
            .contains("this PR touches: demo_ci_app"));
        assert!(labels
            .human_summary()
            .contains("also still drifted: TA-linux"));
    }

    #[test]
    fn env_yaml_change_marks_all_mapped_as_touched() {
        let drifted = vec!["demo_ci_app".into(), "TA-linux".into()];
        let mapped = vec![
            ("apps/demo_ci_app".into(), "demo_ci_app".into()),
            ("apps/TA-linux".into(), "TA-linux".into()),
        ];
        let changed = vec![".deslicer/environments/acme-prod.yml".into()];
        let labels = label_plan_apps(&drifted, &mapped, &changed).expect("labels");
        assert_eq!(labels.pr_touched_apps.len(), 2);
        assert!(labels.also_still_drifted_apps.is_empty());
    }

    #[test]
    fn no_changed_paths_skips_labeling() {
        let drifted = vec!["demo_ci_app".into()];
        assert!(label_plan_apps(&drifted, &[], &[]).is_none());
    }

    #[test]
    fn mapped_apps_from_yaml_use_basename_labels() {
        let yaml = "\
destinations:
  - inventory_group: indexers
    apps:
      - source_path: apps/demo_ci_app
      - source_path: apps/TA-linux
";
        let mapped = mapped_apps_from_yaml(yaml);
        assert_eq!(
            mapped,
            vec![
                ("apps/demo_ci_app".into(), "demo_ci_app".into()),
                ("apps/TA-linux".into(), "TA-linux".into()),
            ]
        );
    }

    #[test]
    fn split_path_list_dedupes() {
        let paths = split_path_list("apps/a\napps/b,apps/a");
        assert_eq!(paths, vec!["apps/a".to_string(), "apps/b".to_string()]);
    }
}
