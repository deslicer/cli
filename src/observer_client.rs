use crate::errors::CliError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePlan {
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanProgress {
    pub id: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ReconcileMode {
    PlanOnly,
    Apply,
}

pub struct Client {
    base: url::Url,
    token: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base: url::Url, token: String) -> Self {
        Self {
            base,
            token,
            http: reqwest::Client::new(),
        }
    }

    pub async fn reconcile(
        &self,
        environment: &Option<String>,
        mode: ReconcileMode,
    ) -> Result<ChangePlan, CliError> {
        let _ = (environment, mode, &self.base, &self.token, &self.http);
        Err(CliError::Other("reconcile not implemented".to_string()))
    }

    pub async fn get_plan(&self, plan_id: &str) -> Result<ChangePlan, CliError> {
        let _ = plan_id;
        Err(CliError::Other("get_plan not implemented".to_string()))
    }

    pub async fn list_plans(&self, environment: Option<&str>) -> Result<Vec<ChangePlan>, CliError> {
        let _ = environment;
        Err(CliError::Other("list_plans not implemented".to_string()))
    }

    pub async fn approve(&self, plan_id: &str) -> Result<ChangePlan, CliError> {
        let _ = plan_id;
        Err(CliError::Other("approve not implemented".to_string()))
    }

    pub async fn reject(&self, plan_id: &str) -> Result<ChangePlan, CliError> {
        let _ = plan_id;
        Err(CliError::Other("reject not implemented".to_string()))
    }

    pub async fn execute(&self, plan_id: &str) -> Result<ChangePlan, CliError> {
        let _ = plan_id;
        Err(CliError::Other("execute not implemented".to_string()))
    }

    pub async fn progress(&self, plan_id: &str) -> Result<PlanProgress, CliError> {
        let _ = plan_id;
        Err(CliError::Other("progress not implemented".to_string()))
    }
}
