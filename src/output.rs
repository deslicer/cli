use crate::ci::{detect_platform, CiPlatform};
use crate::observer_client::{ChangePlan, PlanProgress};
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
    let json = serde_json::to_string(&map)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
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

pub fn emit_change_plan(plan: &ChangePlan) -> i32 {
    println!("{}", serde_json::to_string(plan).unwrap_or_default());
    let pairs = [
        ("plan_id", plan.id.clone()),
        ("plan_status", plan.status.clone()),
        ("plan_summary", plan.summary.clone().unwrap_or_default()),
    ];
    emit_to_sink(&pairs)
}

pub fn emit_plan_progress(progress: &PlanProgress) -> i32 {
    println!("{}", serde_json::to_string(progress).unwrap_or_default());
    let pairs = [
        ("plan_id", progress.id.clone()),
        ("plan_status", progress.status.clone()),
    ];
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
}
