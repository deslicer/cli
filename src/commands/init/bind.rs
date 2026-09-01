use uuid::Uuid;

use crate::commands::pipeline::AuthenticatedSession;
use crate::errors::CliError;
use crate::observer_client::{Client, CreateEnvironmentBindingRequest, GithubInstallation};

use super::provider::{InitProvider, OriginRepo};

/// Sentinel used by Observer for gitlab.com rows (DAI
/// `GITLAB_COM_BINDING_INSTALLATION_ID`). Not a GitHub App installation.
const GITLAB_COM_BINDING_INSTALLATION_ID: i64 = 9_000_000_001;

pub enum BindOutcome {
    Bound { already: bool },
    NeedsGithubConnect,
    PrintPortal { message: String },
}

pub async fn bind_repo(
    client: &Client,
    session: &AuthenticatedSession,
    provider: InitProvider,
    origin: &OriginRepo,
    environment: &str,
    target_group: Uuid,
) -> Result<BindOutcome, CliError> {
    if !session.is_device_session() {
        return Err(CliError::Other(
            "`init --bind` requires `deslicer auth login` (device session)".into(),
        ));
    }
    match provider {
        InitProvider::Github => bind_github(client, origin, environment, target_group).await,
        InitProvider::Gitlab => bind_gitlab(client, origin, environment, target_group).await,
        InitProvider::GithubToken => Ok(BindOutcome::PrintPortal {
            message: path_a2_next_step(),
        }),
        InitProvider::Azure => Ok(BindOutcome::PrintPortal {
            message: "Azure DevOps compile is bundle-only this release. Bind the environment in the portal (Platform → CI environments), then run `deslicer change plan --source-dir .`.".into(),
        }),
        InitProvider::Bitbucket => Ok(BindOutcome::PrintPortal {
            message: "Bitbucket compile is bundle-only this release. Bind the environment in the portal (Platform → CI environments), then run `deslicer change plan --source-dir .`.".into(),
        }),
    }
}

async fn bind_github(
    client: &Client,
    origin: &OriginRepo,
    environment: &str,
    target_group: Uuid,
) -> Result<BindOutcome, CliError> {
    let installations = client.list_github_installations().await?;
    let matches: Vec<&GithubInstallation> = installations
        .iter()
        .filter(|install| {
            install.status != "deleted"
                && install
                    .github_account_login
                    .eq_ignore_ascii_case(&origin.owner)
        })
        .collect();
    if matches.is_empty() {
        return Ok(BindOutcome::NeedsGithubConnect);
    }
    if matches.len() > 1 {
        return Err(CliError::AmbiguousBinding(format!(
            "multiple GitHub App installations cover {}; pick one in the portal",
            origin.owner
        )));
    }
    let installation_id = matches[0].installation_id;
    let already = post_binding(
        client,
        CreateEnvironmentBindingRequest {
            installation_id,
            github_full_name: origin.full_name.clone(),
            environment_name: environment.to_string(),
            ci_platform: "github".into(),
            host_group_id: target_group,
        },
    )
    .await?;
    Ok(BindOutcome::Bound { already })
}

async fn bind_gitlab(
    client: &Client,
    origin: &OriginRepo,
    environment: &str,
    target_group: Uuid,
) -> Result<BindOutcome, CliError> {
    if origin.host != "gitlab.com" && !origin.host.ends_with(".gitlab.com") {
        return Err(CliError::Other(
            "GitLab git-source bind is gitlab.com only this release; self-managed uses --source-dir bundles".into(),
        ));
    }
    let already = match post_binding(
        client,
        CreateEnvironmentBindingRequest {
            installation_id: GITLAB_COM_BINDING_INSTALLATION_ID,
            github_full_name: origin.full_name.clone(),
            environment_name: environment.to_string(),
            ci_platform: "gitlab".into(),
            host_group_id: target_group,
        },
    )
    .await
    {
        Ok(already) => already,
        Err(CliError::BackendUnavailable(_)) => {
            return Err(CliError::Other(
                "Observer could not verify the GitLab.com project. Ask a platform admin to configure the GitLab compile token on Observer, then retry --bind.".into(),
            ));
        }
        Err(other) => return Err(other),
    };
    Ok(BindOutcome::Bound { already })
}

async fn post_binding(
    client: &Client,
    body: CreateEnvironmentBindingRequest,
) -> Result<bool, CliError> {
    client.create_environment_binding(&body).await
}

pub fn bind_next_step(provider: InitProvider) -> String {
    match provider {
        InitProvider::Github => {
            "deslicer init --provider github --bind --environment <name> --target-group <uuid>\n\
             Or connect GitHub in the portal: Platform → GitHub → Connect"
                .into()
        }
        InitProvider::GithubToken => path_a2_next_step(),
        InitProvider::Gitlab => {
            "deslicer init --provider gitlab --bind --environment <name> --target-group <uuid>"
                .into()
        }
        InitProvider::Azure | InitProvider::Bitbucket => {
            "Bind this repo in the portal (Platform → CI environments). Compile is bundle-only: \
             deslicer change plan --source-dir . --environment <name> --target-group <uuid>"
                .into()
        }
    }
}

/// Path A2: git-sourced plan with Observer tools token (no GitHub App bind).
fn path_a2_next_step() -> String {
    "Path A2 (Observer API token, no GitHub App). Set these, then commit the scaffolded workflows:\n\
     - Secret: DESLICER_API_TOKEN (tools-scope Observer key)\n\
     - Variable (or secret): OBSERVER_API_URL\n\
     - Variable: TARGET_GROUP_ID\n\
     - Variable: DESLICER_API_URL (portal base for plan links)\n\
     Re-scaffold: deslicer init --provider github-token --force"
        .into()
}
