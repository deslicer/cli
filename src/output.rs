use crate::ci::CiPlatform;
use crate::observer_client::{ChangePlan, PlanProgress};

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

pub fn emit_change_plan(plan: &ChangePlan) -> i32 {
    println!("{}", serde_json::to_string(plan).unwrap_or_default());
    0
}

pub fn emit_plan_progress(progress: &PlanProgress) -> i32 {
    println!("{}", serde_json::to_string(progress).unwrap_or_default());
    0
}
