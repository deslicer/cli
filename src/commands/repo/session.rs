use crate::ci::CiPlatform;
use crate::commands::pipeline::AuthenticatedSession;
use crate::errors::CliError;

/// Same copy as `change plan` when no device session is present.
pub const NOT_LOGGED_IN: &str =
    "not logged in. Run `deslicer auth login` and approve the code in the portal";

const GITHUB_APP_ONLY: &str =
    "deslicer repo is GitHub App only. GitLab, Azure DevOps, and Bitbucket never call GitHub repo provision. Use `deslicer init --provider` to write local pipeline files.";

pub fn require_repo_session(session: &AuthenticatedSession) -> Result<(), CliError> {
    refuse_non_github_ci(session.platform)?;
    if session.is_device_session() {
        Ok(())
    } else {
        Err(CliError::Other(NOT_LOGGED_IN.to_string()))
    }
}

pub fn refuse_non_github_ci(platform: CiPlatform) -> Result<(), CliError> {
    match platform {
        CiPlatform::Local | CiPlatform::Github => Ok(()),
        CiPlatform::Gitlab | CiPlatform::Azure | CiPlatform::Bitbucket => {
            Err(CliError::UnsupportedPlatform(GITHUB_APP_ONLY.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::ResolvedBackend;
    use url::Url;

    fn session(platform: CiPlatform, resolution_path: &str) -> AuthenticatedSession {
        AuthenticatedSession {
            platform,
            backend: ResolvedBackend {
                observer_api_url: Url::parse("https://observer.example").expect("url"),
                audience: crate::ci::AUDIENCE.to_string(),
                resolution_path: resolution_path.to_string(),
                proxy_mode: true,
            },
        }
    }

    #[test]
    fn gitlab_azure_bitbucket_never_reach_provision() {
        for platform in [CiPlatform::Gitlab, CiPlatform::Azure, CiPlatform::Bitbucket] {
            let err = refuse_non_github_ci(platform).expect_err("refuse");
            let text = err.to_string();
            assert!(text.contains("GitHub App only"), "{text}");
            assert!(!text.contains("9000000001"), "{text}");
            assert!(!text.contains("/admin/github-installations"), "{text}");
        }
    }

    #[test]
    fn local_and_github_are_allowed() {
        refuse_non_github_ci(CiPlatform::Local).expect("local");
        refuse_non_github_ci(CiPlatform::Github).expect("github");
    }

    #[test]
    fn missing_device_session_uses_change_plan_copy() {
        let err = require_repo_session(&session(CiPlatform::Local, "observer_api_token"))
            .expect_err("token");
        assert_eq!(err.to_string(), NOT_LOGGED_IN);
    }

    #[test]
    fn device_session_is_accepted() {
        require_repo_session(&session(CiPlatform::Local, "device_session")).expect("ok");
    }
}
