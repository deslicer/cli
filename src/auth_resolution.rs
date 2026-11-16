use crate::ci::{self, CiPlatform};
use crate::errors::CliError;
use crate::token_store::{load_stored_session, StoredSession};
use crate::Ctx;

const LEGACY_DEV_TOKEN: &str = "DESLICER_DEV_TOKEN";

pub enum AuthCredential {
    DirectObserver { platform: CiPlatform },
    CiOidc { platform: CiPlatform },
    Device(StoredSession),
    ExpiredDevice(StoredSession),
    None,
}

pub struct AuthCredentialResolver<'a> {
    ctx: &'a Ctx,
}

impl<'a> AuthCredentialResolver<'a> {
    pub fn new(ctx: &'a Ctx) -> Self {
        Self { ctx }
    }

    pub fn resolve(&self) -> Result<AuthCredential, CliError> {
        let platform = ci::detect_platform(self.ctx.ci_override);
        if crate::observer_token::direct_auth_ready(self.ctx) {
            return Ok(AuthCredential::DirectObserver { platform });
        }
        if platform != CiPlatform::Local {
            return Ok(AuthCredential::CiOidc { platform });
        }

        if ignore_device_session() {
            if std::env::var_os(LEGACY_DEV_TOKEN).is_some() {
                return Err(legacy_dev_token_error());
            }
            return Ok(AuthCredential::None);
        }

        let stored_session = load_stored_session()?;
        if let Some(session) = stored_session
            .as_ref()
            .filter(|session| session.is_active())
        {
            return Ok(AuthCredential::Device(session.clone()));
        }
        if std::env::var_os(LEGACY_DEV_TOKEN).is_some() {
            return Err(legacy_dev_token_error());
        }
        Ok(match stored_session {
            Some(session) => AuthCredential::ExpiredDevice(session),
            None => AuthCredential::None,
        })
    }
}

/// Skip the kernel keyring and the file credential without deleting them.
///
/// Empty `HOME` does not isolate the kernel keyring. Set this to `1`, `true`,
/// or `yes` for a noninteractive logged-out run (issue #268).
pub fn ignore_device_session() -> bool {
    matches!(
        std::env::var("DESLICER_IGNORE_DEVICE_SESSION") // pragma: allowlist secret
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

pub fn legacy_dev_token_error() -> CliError {
    CliError::Other(
        "DESLICER_DEV_TOKEN has been retired and was not sent. For interactive use, \
         unset it and run `deslicer auth login` to start device login. For automation, \
         unset it and set both OBSERVER_API_URL and DESLICER_API_TOKEN (an Observer \
         tools-scope API key)."
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_flag_reads_true_yes_and_one() {
        let _guard = crate::observer_token::ENV_LOCK.lock().expect("env lock");
        std::env::set_var("DESLICER_IGNORE_DEVICE_SESSION", "true"); // pragma: allowlist secret
        assert!(ignore_device_session());
        std::env::set_var("DESLICER_IGNORE_DEVICE_SESSION", "yes"); // pragma: allowlist secret
        assert!(ignore_device_session());
        std::env::set_var("DESLICER_IGNORE_DEVICE_SESSION", "1"); // pragma: allowlist secret
        assert!(ignore_device_session());
        std::env::set_var("DESLICER_IGNORE_DEVICE_SESSION", "0"); // pragma: allowlist secret
        assert!(!ignore_device_session());
        std::env::remove_var("DESLICER_IGNORE_DEVICE_SESSION"); // pragma: allowlist secret
    }
}
