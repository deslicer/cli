use crate::ci::{detect_platform, CiPlatform};
use crate::diff_summary::DiffCounts;
use crate::observer_client::{ChangePlan, ExecutionQueued, ExecutionSummary, PlanProgress};
use crate::pr_preview::PrPreviewLabels;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub enum OutputSink {
    GithubOutput,
    GitlabDotenv,
    AzureLogging,
    BitbucketStorage,
    Stdout,
}

pub fn detect_sink(platform: CiPlatform) -> OutputSink {
    match platform {
        CiPlatform::Github => OutputSink::GithubOutput,
        CiPlatform::Gitlab => OutputSink::GitlabDotenv,
        CiPlatform::Azure => OutputSink::AzureLogging,
        CiPlatform::Bitbucket => OutputSink::BitbucketStorage,
        CiPlatform::Local => OutputSink::Stdout,
    }
}

fn format_azure_line(key: &str, value: &str) -> String {
    format!("##vso[task.setvariable variable={key}]{value}")
}

fn append_kv_file(path: &str, pairs: &[(&str, String)]) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    for (key, value) in pairs {
        writeln!(file, "{key}={value}")?;
    }
    Ok(())
}

fn write_stdout_json(pairs: &[(&str, String)]) -> io::Result<()> {
    let map: BTreeMap<&str, &str> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let json = serde_json::to_string(&map).map_err(|e| io::Error::other(e.to_string()))?;
    println!("{json}");
    Ok(())
}

fn write_outputs(sink: OutputSink, pairs: &[(&str, String)]) -> io::Result<()> {
    match sink {
        OutputSink::GithubOutput => match std::env::var("GITHUB_OUTPUT") {
            Ok(path) => append_kv_file(&path, pairs),
            Err(_) => write_stdout_json(pairs),
        },
        OutputSink::GitlabDotenv => match std::env::var("DESLICER_DOTENV_PATH") {
            Ok(path) => append_kv_file(&path, pairs),
            Err(_) => write_stdout_json(pairs),
        },
        OutputSink::AzureLogging => {
            let mut stdout = io::stdout();
            for (key, value) in pairs {
                writeln!(stdout, "{}", format_azure_line(key, value))?;
            }
            Ok(())
        }
        OutputSink::BitbucketStorage => {
            let dir = std::env::var("BITBUCKET_PIPE_STORAGE_DIR").unwrap_or_else(|_| ".".into());
            let path = PathBuf::from(dir).join("deslicer-output.env");
            append_kv_file(&path.to_string_lossy(), pairs)
        }
        OutputSink::Stdout => write_stdout_json(pairs),
    }
}

fn append_github_step_summary(markdown: &str) -> io::Result<()> {
    if markdown.trim().is_empty() {
        return Ok(());
    }
    let path = match std::env::var("GITHUB_STEP_SUMMARY") {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{markdown}")?;
    Ok(())
}

fn emit_to_sink(pairs: &[(&str, String)]) -> i32 {
    let platform = detect_platform(None);
    let sink = detect_sink(platform);
    match write_outputs(sink, pairs) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("output write failed: {e}");
            1
        }
    }
}

pub fn emit_message(key_values: &[(&str, String)]) -> i32 {
    emit_to_sink(key_values)
}

pub fn diff_count_pairs(counts: &DiffCounts) -> Vec<(&'static str, String)> {
    vec![
        ("diff_total", counts.total.to_string()),
        ("diff_additions", counts.additions.to_string()),
        ("diff_modifications", counts.modifications.to_string()),
        ("diff_deletions", counts.deletions.to_string()),
        ("diff_has_destructive", counts.has_destructive.to_string()),
    ]
}

pub fn emit_diff_counts(counts: &DiffCounts) -> i32 {
    emit_to_sink(&diff_count_pairs(counts))
}

fn plan_summary_markdown(
    title: &str,
    plan: &ChangePlan,
    diff: Option<&DiffCounts>,
    preview: Option<&PrPreviewLabels>,
) -> String {
    let mut lines = vec![
        format!("## {title}"),
        String::new(),
        format!("| Field | Value |"),
        format!("| --- | --- |"),
        format!("| Plan ID | `{}` |", plan.external_id()),
        format!("| Row ID | `{}` |", plan.id),
        format!("| Status | **{}** |", plan.status),
    ];
    let summary = plan.display_summary();
    if !summary.is_empty() {
        lines.push(format!("| Summary | {summary} |"));
    }
    if let Some(counts) = diff {
        lines.push(format!(
            "| Changes | {} (+{} / ~{} / -{}) |",
            counts.total, counts.additions, counts.modifications, counts.deletions
        ));
        if counts.has_destructive {
            lines.push("| Destructive | yes |".to_string());
        }
    }
    let mut body = lines.join("\n");
    if let Some(labels) = preview {
        body.push_str("\n\n");
        body.push_str(&labels.markdown_section());
    }
    body
}

pub fn pr_preview_pairs(labels: &PrPreviewLabels) -> Vec<(&'static str, String)> {
    vec![
        ("pr_touched_apps", labels.pr_touched_apps.join(",")),
        (
            "also_still_drifted_apps",
            labels.also_still_drifted_apps.join(","),
        ),
        ("pr_preview_summary", labels.human_summary()),
    ]
}

pub fn emit_change_plan(plan: &ChangePlan) -> i32 {
    emit_change_plan_with_diff(plan, None, None)
}

pub fn emit_change_plan_with_diff(
    plan: &ChangePlan,
    diff: Option<&DiffCounts>,
    preview: Option<&PrPreviewLabels>,
) -> i32 {
    println!("{}", serde_json::to_string(plan).unwrap_or_default());
    let summary = if let Some(labels) = preview.filter(|value| !value.is_empty()) {
        let base = if let Some(counts) = diff {
            counts.human_summary()
        } else {
            plan.display_summary()
        };
        if base.is_empty() {
            labels.human_summary()
        } else {
            format!("{base}; {}", labels.human_summary())
        }
    } else if let Some(counts) = diff {
        counts.human_summary()
    } else {
        plan.display_summary()
    };
    let mut pairs = vec![
        ("plan_id", plan.external_id().to_string()),
        ("plan_row_id", plan.id.clone()),
        ("plan_status", plan.status.clone()),
        ("plan_summary", summary),
    ];
    if let Some(counts) = diff {
        pairs.extend(diff_count_pairs(counts));
    }
    if let Some(labels) = preview {
        pairs.extend(pr_preview_pairs(labels));
    }
    let _ =
        append_github_step_summary(&plan_summary_markdown("Deslicer plan", plan, diff, preview));
    emit_to_sink(&pairs)
}

pub fn emit_change_plans(plans: &[ChangePlan]) -> i32 {
    if let [plan] = plans {
        return emit_change_plan(plan);
    }
    println!("{}", serde_json::to_string(plans).unwrap_or_default());
    let plan_ids: Vec<String> = plans
        .iter()
        .map(|plan| plan.external_id().to_string())
        .collect();
    let first = plans.first();
    let pairs = vec![
        (
            "plan_id",
            first
                .map(ChangePlan::external_id)
                .unwrap_or_default()
                .to_string(),
        ),
        ("plan_ids", plan_ids.join(",")),
        ("plan_count", plans.len().to_string()),
        (
            "plan_status",
            first.map(|plan| plan.status.clone()).unwrap_or_default(),
        ),
    ];
    let _ = append_github_step_summary(&plans_summary_markdown(plans));
    emit_to_sink(&pairs)
}

fn plans_summary_markdown(plans: &[ChangePlan]) -> String {
    let mut lines = vec![
        "## Deslicer plans".to_string(),
        String::new(),
        "| Environment plan | Status |".to_string(),
        "| --- | --- |".to_string(),
    ];
    for plan in plans {
        lines.push(format!(
            "| `{}` | **{}** |",
            plan.external_id(),
            plan.status
        ));
    }
    lines.join("\n")
}

pub fn emit_plan_progress(progress: &PlanProgress) -> i32 {
    emit_plan_status(None, progress, None)
}

pub fn emit_plan_status(
    plan: Option<&ChangePlan>,
    progress: &PlanProgress,
    diff: Option<&DiffCounts>,
) -> i32 {
    println!("{}", serde_json::to_string(progress).unwrap_or_default());
    let plan_status = plan.map(|p| p.status.as_str()).unwrap_or_default();
    let plan_summary = plan.map(ChangePlan::display_summary).unwrap_or_default();
    let mut pairs = vec![
        ("plan_id", progress.plan_id.clone()),
        ("progress_status", progress.progress_status.clone()),
        ("total_items", progress.total_items.to_string()),
        (
            "fully_completed_items",
            progress.fully_completed_items.to_string(),
        ),
        ("plan_status", plan_status.to_string()),
        ("plan_summary", plan_summary),
    ];
    if let Some(counts) = diff {
        pairs.extend(diff_count_pairs(counts));
    }
    if let Some(plan) = plan {
        let _ = append_github_step_summary(&plan_summary_markdown(
            "Deslicer plan status",
            plan,
            diff,
            None,
        ));
    }
    emit_to_sink(&pairs)
}

pub fn emit_execution_queued(execution: &ExecutionQueued) -> i32 {
    println!("{}", serde_json::to_string(execution).unwrap_or_default());
    let pairs = [
        ("execution_id", execution.execution_id.clone()),
        ("execution_status", execution.status.clone()),
        ("jobs_total", execution.jobs_total.to_string()),
        ("plan_id", execution.plan_id.clone().unwrap_or_default()),
    ];
    let summary = format!(
        "## Deslicer deploy queued\n\n| Field | Value |\n| --- | --- |\n| Execution | `{}` |\n| Status | **{}** |\n| Jobs | {} |",
        execution.execution_id, execution.status, execution.jobs_total
    );
    let _ = append_github_step_summary(&summary);
    emit_to_sink(&pairs)
}

pub fn emit_execution_summary(summary: &ExecutionSummary) -> i32 {
    println!("{}", serde_json::to_string(summary).unwrap_or_default());
    let pairs = [
        ("execution_id", summary.execution_id.clone()),
        ("execution_status", summary.status.clone()),
        ("jobs_total", summary.jobs_total.to_string()),
        ("jobs_succeeded", summary.jobs_succeeded.to_string()),
        ("jobs_failed", summary.jobs_failed.to_string()),
    ];
    let md = format!(
        "## Deslicer deploy result\n\n| Field | Value |\n| --- | --- |\n| Execution | `{}` |\n| Status | **{}** |\n| Jobs | {}/{} succeeded |",
        summary.execution_id,
        summary.status,
        summary.jobs_succeeded,
        summary.jobs_total
    );
    let _ = append_github_step_summary(&md);
    emit_to_sink(&pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn format_azure_line_sets_task_variable() {
        let line = format_azure_line("plan_id", "abc-123");
        assert_eq!(line, "##vso[task.setvariable variable=plan_id]abc-123");
    }

    #[test]
    fn github_output_appends_kv() {
        let _guard = ENV_LOCK.lock().unwrap();
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_string_lossy().into_owned();
        std::env::set_var("GITHUB_OUTPUT", &path);
        let pairs = [("plan_id", "gh-plan".to_string())];
        write_outputs(OutputSink::GithubOutput, &pairs).unwrap();
        std::env::remove_var("GITHUB_OUTPUT");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("plan_id=gh-plan"));
    }

    #[test]
    fn gitlab_dotenv_appends_kv() {
        let _guard = ENV_LOCK.lock().unwrap();
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_string_lossy().into_owned();
        std::env::set_var("DESLICER_DOTENV_PATH", &path);
        let pairs = [("plan_status", "approved".to_string())];
        write_outputs(OutputSink::GitlabDotenv, &pairs).unwrap();
        std::env::remove_var("DESLICER_DOTENV_PATH");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("plan_status=approved"));
    }

    #[test]
    fn github_missing_env_falls_back_to_stdout_json() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("GITHUB_OUTPUT");
        let pairs = [("plan_id", "fallback".to_string())];
        write_outputs(OutputSink::GithubOutput, &pairs).unwrap();
    }

    #[test]
    fn step_summary_appends_markdown() {
        let _guard = ENV_LOCK.lock().unwrap();
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_string_lossy().into_owned();
        std::env::set_var("GITHUB_STEP_SUMMARY", &path);
        let plan = ChangePlan {
            id: "row".into(),
            plan_id: Some("ext".into()),
            status: "pending_approval".into(),
            name: None,
            summary: None,
        };
        let _ = append_github_step_summary(&plan_summary_markdown("Test", &plan, None, None));
        std::env::remove_var("GITHUB_STEP_SUMMARY");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("pending_approval"));
        assert!(content.contains("ext"));
    }

    #[test]
    fn step_summary_includes_pr_preview_labels() {
        let labels = PrPreviewLabels {
            pr_touched_apps: vec!["demo_ci_app".into()],
            also_still_drifted_apps: vec!["TA-linux".into()],
        };
        let plan = ChangePlan {
            id: "row".into(),
            plan_id: Some("ext".into()),
            status: "pending_approval".into(),
            name: None,
            summary: None,
        };
        let markdown = plan_summary_markdown("Test", &plan, None, Some(&labels));
        assert!(markdown.contains("This PR touches"));
        assert!(markdown.contains("demo_ci_app"));
        assert!(markdown.contains("Also still drifted"));
        assert!(markdown.contains("TA-linux"));
    }

    #[test]
    fn plans_summary_lists_each_plan() {
        let plans = [
            ChangePlan {
                id: "row-1".into(),
                plan_id: Some("ext-1".into()),
                status: "draft".into(),
                name: None,
                summary: None,
            },
            ChangePlan {
                id: "row-2".into(),
                plan_id: Some("ext-2".into()),
                status: "pending_approval".into(),
                name: None,
                summary: None,
            },
        ];
        let markdown = plans_summary_markdown(&plans);
        assert!(markdown.contains("ext-1"));
        assert!(markdown.contains("ext-2"));
    }
}
