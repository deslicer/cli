//! Resolve `--environment` for token-path `change plan`.

use std::path::{Path, PathBuf};

use crate::environment_paths::{resolve_optional_environment_stem, search_roots_for};
use crate::errors::CliError;
use crate::token_store::load_active_session;
use crate::Ctx;

/// Token path: derive a stem from `--environment`, session slug, or a single env file.
/// Other auth paths pass the flag through unchanged.
pub fn resolve_plan_environment(
    ctx: &Ctx,
    explicit: Option<&str>,
) -> Result<Option<String>, CliError> {
    if !crate::observer_token::direct_auth_ready(ctx) {
        return Ok(explicit.map(str::to_string));
    }
    let tenant_slug = load_active_session()?.and_then(|session| session.tenant_slug);
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let roots = search_roots_for(&cwd);
    let refs: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    Ok(
        resolve_optional_environment_stem(explicit, tenant_slug.as_deref(), &refs)?
            .map(|resolved| resolved.stem),
    )
}

#[cfg(test)]
mod tests {
    use super::resolve_plan_environment;
    use crate::ci::CiPlatform;
    use crate::cli::LogFormat;
    use crate::Ctx;
    use url::Url;

    fn ctx_without_observer() -> Ctx {
        Ctx {
            deslicer_api_url: Url::parse("https://api.deslicer.ai").expect("url"),
            observer_api_url: None,
            ci_override: Some(CiPlatform::Local),
            log_format: LogFormat::Human,
        }
    }

    #[test]
    fn non_token_path_passes_environment_through() {
        let resolved = resolve_plan_environment(&ctx_without_observer(), Some("prod")).unwrap();
        assert_eq!(resolved.as_deref(), Some("prod"));
        assert!(resolve_plan_environment(&ctx_without_observer(), None)
            .unwrap()
            .is_none());
    }
}
