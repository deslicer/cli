//! Read-only description of bootstrap auth (#120).
//!
//! The CLI does not create GitHub credentials, push git, or write Environment
//! secrets. This command prints the two supported paths so agents do not invent
//! a third one.

use clap::Args as ClapArgs;
use serde_json::{json, Value};

use crate::cli::LogFormat;
use crate::Ctx;

#[derive(ClapArgs)]
pub struct Args {}

pub fn contract() -> Value {
    json!({
        "lab": {
            "auth": "operator_github_token",
            "cli_stores_github_token": false,
            "create_or_link": "operator creates the repo, or uses repo bootstrap when an App installation already exists",
            "push": "operator git push with their own credential",
            "workflow_trigger": "operator dispatches the workflow after secrets exist"
        },
        "production": {
            "auth": "github_app_via_observer",
            "cli_receives_app_private_key": false,
            "create_or_link": "repo bootstrap (dry-run unless --yes) through the portal GitHub App installation",
            "push": "not performed by the CLI",
            "workflow_refresh": "repo refresh opens a pull request; it does not dispatch a workflow",
            "secrets": "a person sets GitHub Environment secrets; the CLI never writes them"
        },
        "required_secret_names": ["API URL", "tools API token"],
        "cli_writes_secrets": false,
        "cli_pushes_git": false,
        "out_of_scope": [
            "DAP workflow reconcile",
            "compiler short-lived per-repo tokens"
        ]
    })
}

pub fn run(ctx: Ctx, _args: Args) -> i32 {
    let body = contract();
    match ctx.log_format {
        LogFormat::Json => crate::json_doc::print_success(&body),
        LogFormat::Human => {
            println!(
                "Bootstrap auth\n\
                 \n\
                 Labs: the operator uses their own GitHub token. The CLI does not store it,\n\
                 push the first commit, or dispatch the workflow.\n\
                 \n\
                 Production: repo bootstrap creates or links the repo through the GitHub App\n\
                 installation Observer already has. The CLI never sees the App private key.\n\
                 A person sets the API URL and tools token as GitHub Environment secrets.\n\
                 repo refresh opens a pull request; it does not dispatch a workflow.\n\
                 \n\
                 Readiness: repo bootstrap --check --installation <id>."
            );
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn contract_never_writes_secrets_or_pushes() {
        let doc = contract();
        assert_eq!(doc["cli_writes_secrets"], false);
        assert_eq!(doc["cli_pushes_git"], false);
        assert_eq!(doc["production"]["cli_receives_app_private_key"], false);
        assert_eq!(doc["lab"]["cli_stores_github_token"], false);
        let text = doc.to_string();
        assert!(!text.contains("ghp_"));
        assert!(!text.contains("github_pat_"));
    }

    #[test]
    fn auth_parses_as_a_repo_subcommand() {
        let cli = crate::cli::Cli::try_parse_from(["cli", "repo", "auth"]).expect("parse");
        assert!(matches!(
            cli.command,
            crate::cli::Command::Repo(super::super::RepoCmd::Auth(_))
        ));
    }
}
