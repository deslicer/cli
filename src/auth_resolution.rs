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

pub fn legacy_dev_token_error() -> CliError {
    CliError::Other(
        "DESLICER_DEV_TOKEN has been retired and was not sent. For interactive use, \
         unset it and run `deslicer auth login` to start device login. For automation, \
         unset it and set both OBSERVER_API_URL and DESLICER_API_TOKEN (an Observer \
         tools-scope API key)."
            .into(),
    )
}
