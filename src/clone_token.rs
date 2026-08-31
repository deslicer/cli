//! Job-scoped git credential forwarded to Observer for one compile run.
//!
//! When the CLI authenticates to Observer with `DESLICER_API_TOKEN` there is no
//! GitHub App installation behind the request, so Observer has no credential of
//! its own to clone a private repository with. Rather than falling back to
//! uploading a tarball — which ships git-lfs *pointer files* instead of their
//! contents — the CLI forwards the credential the CI job already holds
//! (`GITHUB_TOKEN`) and Observer uses it for that single clone.
//!
//! REQ-LOG-007 / REQ-SEC-005: env-only so the value never lands in process argv,
//! and `Debug` renders `[REDACTED]` so it cannot escape through a log line.

use crate::ci::CiPlatform;

/// Explicit override, for runners whose job token is not `GITHUB_TOKEN`.
const OVERRIDE_ENV: &str = "DESLICER_GIT_CLONE_TOKEN";
/// The token GitHub Actions injects into every job.
const GITHUB_ENV: &str = "GITHUB_TOKEN";

const REDACTED: &str = "[REDACTED]";

/// A single-use git credential destined for the ephemeral compile-runner.
#[derive(Clone, PartialEq, Eq)]
pub struct CloneToken(String);

impl CloneToken {
    /// Borrow the secret for serialisation into the compile request body.
    ///
    /// Callers must not log it or place it on a command line.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    /// Accept a value only if it is shaped like a credential.
    ///
    /// Observer re-validates at its own intake boundary; checking here turns a
    /// malformed env var into an immediate, readable local failure instead of a
    /// `400 invalid_clone_token` after the plan row has already been created.
    fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_graphic()) {
            return None;
        }
        Some(Self(trimmed.to_string()))
    }
}

impl std::fmt::Debug for CloneToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(REDACTED)
    }
}

/// Resolve the job-scoped clone credential for `platform`, if one is available.
///
/// `DESLICER_GIT_CLONE_TOKEN` always wins so an operator can supply a
/// longer-lived credential for a repository the job token cannot read (a
/// separate config repo, for instance).
///
/// `GITHUB_TOKEN` is only read on GitHub-flavoured runners. Observer accepts a
/// caller-supplied credential for GitHub remotes only — GitLab job tokens need a
/// different HTTPS username, so forwarding `CI_JOB_TOKEN` would authenticate
/// incorrectly rather than fail loudly.
pub fn from_env(platform: CiPlatform) -> Option<CloneToken> {
    if let Some(token) = std::env::var(OVERRIDE_ENV)
        .ok()
        .as_deref()
        .and_then(CloneToken::parse)
    {
        return Some(token);
    }
    match platform {
        // `Local` falls back to GitHub identity resolution, so honour the same env.
        CiPlatform::Github | CiPlatform::Local => std::env::var(GITHUB_ENV)
            .ok()
            .as_deref()
            .and_then(CloneToken::parse),
        CiPlatform::Gitlab | CiPlatform::Azure | CiPlatform::Bitbucket => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn clear() {
        std::env::remove_var(OVERRIDE_ENV);
        std::env::remove_var(GITHUB_ENV);
    }

    #[test]
    fn debug_never_reveals_the_secret() {
        let rendered = format!("{:?}", CloneToken("ghs_supersecretvalue".into()));
        assert_eq!(rendered, REDACTED);
    }

    #[test]
    fn override_wins_over_the_github_job_token() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear();
        std::env::set_var(GITHUB_ENV, "ghs_jobtoken");
        std::env::set_var(OVERRIDE_ENV, "ghp_operatortoken");
        let token = from_env(CiPlatform::Github).expect("token");
        assert_eq!(token.expose_secret(), "ghp_operatortoken");
        clear();
    }

    #[test]
    fn github_job_token_is_used_when_no_override_is_set() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear();
        std::env::set_var(GITHUB_ENV, "ghs_jobtoken");
        assert_eq!(
            from_env(CiPlatform::Github).expect("token").expose_secret(),
            "ghs_jobtoken"
        );
        clear();
    }

    #[test]
    fn gitlab_does_not_forward_a_credential() {
        // A GitLab job token needs the `gitlab-ci-token` HTTPS username, which
        // Observer's caller-token path does not model.
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear();
        std::env::set_var(GITHUB_ENV, "ghs_jobtoken");
        assert!(from_env(CiPlatform::Gitlab).is_none());
        clear();
    }

    #[test]
    fn gitlab_still_honours_an_explicit_override() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear();
        std::env::set_var(OVERRIDE_ENV, "ghp_operatortoken");
        assert!(from_env(CiPlatform::Gitlab).is_some());
        clear();
    }

    #[test]
    fn blank_and_malformed_values_resolve_to_none() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        for raw in ["", "   ", "ghs_abc def", "ghs_abc\ndef", "ghs_abc\tdef"] {
            clear();
            std::env::set_var(GITHUB_ENV, raw);
            assert!(
                from_env(CiPlatform::Github).is_none(),
                "accepted malformed token"
            );
        }
        clear();
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear();
        std::env::set_var(GITHUB_ENV, "  ghs_jobtoken\n");
        assert_eq!(
            from_env(CiPlatform::Github).expect("token").expose_secret(),
            "ghs_jobtoken"
        );
        clear();
    }
}
