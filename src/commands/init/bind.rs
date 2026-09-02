use uuid::Uuid;

use crate::cli::LogFormat;
use crate::commands::pipeline::AuthenticatedSession;
use crate::errors::CliError;
use crate::observer_client::{Client, CreateEnvironmentBindingRequest, GithubInstallation};
use crate::Ctx;

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
pub fn print_bind_outcome(ctx: &Ctx, outcome: &BindOutcome) {
    match outcome {
        BindOutcome::Bound { already } => {
            if matches!(ctx.log_format, LogFormat::Json) {
                println!(
                    "{}",
                    serde_json::json!({ "bound": true, "already": already })
                );
                return;
            }
            if *already {
                println!("Environment already bound.");
            } else {
                println!("Environment binding created.");
            }
        }
        BindOutcome::NeedsGithubConnect => {
            println!("No GitHub App installation covers this org.");
            println!("Connect GitHub in the portal: Platform → GitHub → Connect");
            println!("Files were still written.");
        }
        BindOutcome::PrintPortal { message } => {
            println!("{message}");
        }
    }
}

fn path_a2_next_step() -> String {
    "Path A2 (Observer API token, no GitHub App). One GitHub Environment per tenant\n\
     (CLI does not create it or write secrets). After init, run the printed `gh` recipe,\n\
     then commit the scaffolded workflows:\n\
     - GitHub Environment named after the tenant slug (same as the YAML stem)\n\
     - Environment secret: DESLICER_API_TOKEN (tools-scope Observer key)\n\
     - Environment variable: OBSERVER_API_URL, DESLICER_ENVIRONMENT\n\
     - Optional: TARGET_GROUP_ID only if workflows still pass a UUID; prefer\n\
       `deslicer change plan --target-group <inventory_group name>`\n\
     - Repo-level variable DESLICER_ENVIRONMENT: name pointer so pull_request can select the Environment\n\
     - Repo-level variable DESLICER_API_URL: portal base for plan links\n\
     A second Observer backend is a second Environment plus a matrix row — not a second repo secret.\n\
     Re-scaffold: deslicer init --provider github-token --force\n\
     Refresh inventory groups: deslicer inventory sync\n\
     Docs: deslicer docs path-a2"
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_a2_points_at_docs_command() {
        let text = bind_next_step(InitProvider::GithubToken);
        assert!(text.contains("deslicer docs path-a2"));
        assert!(text.contains("DESLICER_API_TOKEN"));
        assert!(text.contains("DESLICER_ENVIRONMENT"));
        assert!(text.contains("GitHub Environment"));
        assert!(text.contains("deslicer inventory sync"));
        assert!(!text.contains("id-token: write"));
    }
}
