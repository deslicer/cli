//! GitHub App bootstrap readiness checks shared by `repo bootstrap --check`
//! and `deslicer doctor` (#187, #120). // pragma: allowlist secret

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::errors::CliError;
use crate::observer_client::{Client, GithubRepoRow, ListReposResponse};

use super::bootstrap::find_installation_org;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CheckItem {
    pub id: &'static str,
    pub ok: bool,
    pub detail: String,
}

impl CheckItem {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "ok": self.ok,
            "detail": self.detail,
        })
    }
}

pub(crate) struct Query {
    pub installation: i64,
    pub name: Option<String>,
    pub repo_id: Option<i64>,
    pub dir: Option<PathBuf>,
}

pub(crate) struct Report {
    pub org: Option<String>,
    pub checks: Vec<CheckItem>,
}

pub(crate) async fn collect(client: &Client, query: Query) -> Result<Report, CliError> {
    let mut checks = vec![CheckItem {
        id: "device_session",
        ok: true,
        detail: "device session active (deslicer auth login)".into(), // pragma: allowlist secret
    }];

    let installations = client.list_github_installations().await?;
    let org = match find_installation_org(&installations, query.installation) {
        Ok(org) => {
            checks.push(CheckItem {
                id: "installation",
                ok: true,
                detail: format!("installation {} org {org}", query.installation),
            });
            Some(org)
        }
        Err(err) => {
            checks.push(CheckItem {
                id: "installation",
                ok: false,
                detail: err.to_string(),
            });
            None
        }
    };

    if org.is_some() {
        let listed = client.list_github_repos(query.installation).await?;
        checks.extend(repo_readiness_checks(
            &listed,
            query.name.as_deref(),
            query.repo_id,
        ));
    }

    checks.push(match query.dir.as_deref() {
        Some(dir) => local_oidc_workflow_check(dir),
        None => CheckItem {
            id: "local_oidc_workflows",
            ok: true,
            detail: "skipped (pass --dir <checkout> to inspect Path A id-token permissions)".into(),
        },
    });
    checks.push(manual_secrets_check());

    Ok(Report { org, checks })
}

fn manual_secrets_check() -> CheckItem {
    CheckItem {
        id: "secrets_and_push",
        ok: true,
        detail:
            "manual: CLI does not write GitHub Environment secrets or push git content; \
                 use portal/`gh` for DESLICER_* secrets and operator git credentials for first push" // pragma: allowlist secret
                .into(),
    }
}

fn repo_readiness_checks(
    listed: &ListReposResponse,
    name: Option<&str>,
    repo_id: Option<i64>,
) -> Vec<CheckItem> {
    if listed.repos.is_empty() {
        return vec![CheckItem {
            id: "repos_linked",
            ok: false,
            detail: "no repositories linked to this installation; run bootstrap --yes or link in the portal"
                .into(),
        }];
    }

    let mut checks = vec![CheckItem {
        id: "repos_linked",
        ok: true,
        detail: format!("{} linked repository(ies)", listed.repos.len()),
    }];
    match select_repo(&listed.repos, name, repo_id) {
        Ok(Some(repo)) => checks.extend(repo_focus_checks(repo)),
        Ok(None) => checks.push(CheckItem {
            id: "repo_focus",
            ok: true,
            detail: "no --name/--repo-id; installation-level checks only".into(),
        }),
        Err(detail) => checks.push(CheckItem {
            id: "repo_focus",
            ok: false,
            detail,
        }),
    }
    checks
}

fn select_repo<'a>(
    repos: &'a [GithubRepoRow],
    name: Option<&str>,
    repo_id: Option<i64>,
) -> Result<Option<&'a GithubRepoRow>, String> {
    if name.is_none() && repo_id.is_none() {
        return Ok(None);
    }
    let matches: Vec<_> = repos
        .iter()
        .filter(|repo| {
            let id_ok = repo_id.is_none_or(|id| repo.github_repo_id == id);
            let name_ok = name.is_none_or(|want| {
                repo.github_full_name == want
                    || repo
                        .github_full_name
                        .rsplit_once('/')
                        .is_some_and(|(_, short)| short == want)
            });
            id_ok && name_ok
        })
        .collect();
    match matches.as_slice() {
        [repo] => Ok(Some(repo)),
        [] => Err(
            "no linked repository matched --name/--repo-id; check `deslicer repo status`".into(), // pragma: allowlist secret
        ),
        _ => Err("multiple linked repositories matched --name/--repo-id; pass --repo-id".into()),
    }
}

fn repo_focus_checks(repo: &GithubRepoRow) -> Vec<CheckItem> {
    let bootstrap_state = repo.bootstrap_pr_state.as_deref().unwrap_or("missing");
    let lowered = bootstrap_state.to_ascii_lowercase();
    let bootstrap_ok = matches!(lowered.as_str(), "merged" | "closed" | "missing" | "-" | "");
    let bootstrap = CheckItem {
        id: "bootstrap_pr",
        ok: bootstrap_ok,
        detail: match &repo.bootstrap_pr_url {
            Some(url) if !bootstrap_ok => {
                format!("bootstrap_pr_state={bootstrap_state} (merge before CI) {url}")
            }
            Some(url) => format!("bootstrap_pr_state={bootstrap_state} {url}"),
            None if bootstrap_ok => format!("bootstrap_pr_state={bootstrap_state}"),
            None => format!(
                "bootstrap_pr_state={bootstrap_state}; open/merge the bootstrap PR before CI"
            ),
        },
    };

    let in_sync = repo.workflows_in_sync.unwrap_or(false);
    let refresh_pending = repo.workflow_refresh_pending;
    let sync_ok = in_sync && !refresh_pending;
    let sync = CheckItem {
        id: "workflows_in_sync",
        ok: sync_ok,
        detail: if sync_ok {
            format!(
                "{} workflows in sync (digest {})",
                repo.github_full_name,
                repo.workflow_template_digest.as_deref().unwrap_or("-")
            )
        } else {
            format!(
                "{} workflows_in_sync={:?} refresh_pending={refresh_pending}; run `deslicer repo refresh`", // pragma: allowlist secret
                repo.github_full_name, repo.workflows_in_sync
            )
        },
    };
    vec![bootstrap, sync]
}

pub(crate) fn local_oidc_workflow_check(dir: &Path) -> CheckItem {
    let workflows = dir.join(".github").join("workflows");
    if !workflows.is_dir() {
        return CheckItem {
            id: "local_oidc_workflows",
            ok: false,
            detail: format!(
                "no .github/workflows under {}; run `deslicer init` or clone the bootstrap repo", // pragma: allowlist secret
                dir.display()
            ),
        };
    }

    let mut saw_id_token = false;
    let mut saw_audience = false;
    const AUDIENCE: &str = "https://api.deslicer.ai"; // pragma: allowlist secret
    let entries = match fs::read_dir(&workflows) {
        Ok(entries) => entries,
        Err(err) => {
            return CheckItem {
                id: "local_oidc_workflows",
                ok: false,
                detail: format!("cannot read {}: {err}", workflows.display()),
            };
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !matches!(ext, "yml" | "yaml") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let lowered = text.to_ascii_lowercase();
        if lowered.contains("id-token") && lowered.contains("write") {
            saw_id_token = true;
        }
        if text.contains(AUDIENCE) {
            saw_audience = true;
        }
    }

    if saw_id_token && saw_audience {
        CheckItem {
            id: "local_oidc_workflows",
            ok: true,
            detail: "Path A OIDC surface found (id-token: write + api.deslicer.ai audience)".into(), // pragma: allowlist secret
        }
    } else if saw_id_token {
        CheckItem {
            id: "local_oidc_workflows",
            ok: true,
            detail: "id-token: write found; confirm audience https://api.deslicer.ai in the job" // pragma: allowlist secret
                .into(),
        }
    } else {
        CheckItem {
            id: "local_oidc_workflows",
            ok: false,
            detail: "no workflow with permissions.id-token: write; Path A CI OIDC will fail".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(in_sync: bool, pending: bool) -> GithubRepoRow {
        GithubRepoRow {
            github_repo_id: 9,
            github_full_name: "acme/splunk-config".into(),
            added_at: "t".into(),
            bootstrap_pr_url: None,
            bootstrap_pr_state: Some("merged".into()),
            workflow_template_digest: Some("abc".into()),
            workflow_drift_checked_at: None,
            workflows_in_sync: Some(in_sync),
            workflow_refresh_pending: pending,
        }
    }

    #[test]
    fn select_repo_matches_short_and_full_name() {
        let repos = vec![repo(true, false)];
        let hit = select_repo(&repos, Some("splunk-config"), None)
            .unwrap()
            .unwrap();
        assert_eq!(hit.github_repo_id, 9);
        let hit = select_repo(&repos, Some("acme/splunk-config"), Some(9))
            .unwrap()
            .unwrap();
        assert_eq!(hit.github_full_name, "acme/splunk-config");
    }

    #[test]
    fn repo_focus_checks_flag_workflow_drift() {
        let checks = repo_focus_checks(&repo(false, true));
        assert!(checks.iter().any(|c| c.id == "workflows_in_sync" && !c.ok));
    }

    #[test]
    fn local_oidc_check_reads_workflow_permissions() {
        let dir = tempfile::tempdir().expect("temp");
        let wf = dir.path().join(".github").join("workflows");
        fs::create_dir_all(&wf).unwrap();
        fs::write(
            wf.join("plan.yml"),
            "permissions:\n  id-token: write\nenv:\n  AUD: https://api.deslicer.ai\n", // pragma: allowlist secret
        )
        .unwrap();
        let check = local_oidc_workflow_check(dir.path());
        assert!(check.ok, "{}", check.detail);
    }
}
