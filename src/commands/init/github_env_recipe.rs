//! Print-only GitHub Environment setup for Path A2.
//!
//! The CLI never creates Environments or writes secrets. Operators run these
//! `gh` commands with their own repo-admin session.

use crate::commands::init::provider::OriginRepo;
use crate::environment_name::is_valid_environment_name;

/// Human-readable recipe. `repo` is `owner/name` when origin is known.
pub fn github_environment_recipe(stem: &str, repo: Option<&str>) -> String {
    let stem = recipe_stem(stem);
    let repo = repo.unwrap_or("$OWNER/$REPO");
    format!(
        "Create a GitHub Environment for this tenant (print-only; not executed):\n\
         \n\
         gh api -X PUT \"repos/{repo}/environments/{stem}\"\n\
         printf '%s' \"$DESLICER_API_TOKEN\" | gh secret set DESLICER_API_TOKEN --env {stem} --repo {repo}\n\
         printf '%s' \"$OBSERVER_API_URL\" | gh variable set OBSERVER_API_URL --env {stem} --repo {repo}\n\
         printf '%s' \"{stem}\" | gh variable set DESLICER_ENVIRONMENT --env {stem} --repo {repo}\n\
         # Optional: only needed for single-group tenants that still pass a UUID.\n\
         # Prefer: deslicer change plan --target-group <inventory_group name>\n\
         # printf '%s' \"$TARGET_GROUP_ID\" | gh variable set TARGET_GROUP_ID --env {stem} --repo {repo}\n\
         printf '%s' \"{stem}\" | gh variable set DESLICER_ENVIRONMENT --repo {repo}\n\
         \n\
         The last line is the repo-level name so pull_request jobs can select this\n\
         Environment. For a second Observer backend, create another Environment and\n\
         add its slug to the plan workflow matrix (see the token-path README)."
    )
}

fn recipe_stem(stem: &str) -> &str {
    if is_valid_environment_name(stem) {
        stem
    } else {
        "<tenant-slug>"
    }
}

pub fn origin_repo_slug(origin: Option<&OriginRepo>) -> Option<String> {
    origin.map(|row| {
        if row.full_name.contains('/') {
            row.full_name.clone()
        } else {
            format!("{}/{}", row.owner, row.full_name)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_uses_stdin_secret_and_env_flag() {
        let text = github_environment_recipe("acme-prod", Some("deslicer/cfg"));
        assert!(text.contains("gh api -X PUT \"repos/deslicer/cfg/environments/acme-prod\""));
        assert!(text.contains("gh secret set DESLICER_API_TOKEN --env acme-prod"));
        assert!(text.contains("printf '%s' \"$DESLICER_API_TOKEN\""));
        assert!(!text.contains("dslk_"));
        assert!(text.contains("workflow matrix"));
    }

    #[test]
    fn invalid_stem_is_not_interpolated() {
        let text = github_environment_recipe("../evil", Some("deslicer/cfg"));
        assert!(text.contains("environments/<tenant-slug>"));
        assert!(!text.contains("../evil"));
    }
}
