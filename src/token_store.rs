//! Persist the device-flow CLI session (keychain, with a 0600 file fallback).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::errors::CliError;

const SERVICE: &str = "deslicer-cli";
const ACCOUNT: &str = "device-session";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredSession {
    pub cli_session_token: String,
    pub expires_at: String,
    pub tenant_id: String,
    pub display_name: String,
    pub observer_api_url: String,
    /// Optional tenant slug for Path A2 environment-file stems.
    /// Old sessions omit the field; serde defaults to `None`.
    #[serde(default)]
    pub tenant_slug: Option<String>,
    /// Portal that issued this session (`--deslicer-api-url` at login).
    /// Old sessions omit the field; the origin of `observer_api_url` is used.
    #[serde(default)]
    pub deslicer_api_url: Option<String>,
}

impl StoredSession {
    pub fn is_active(&self) -> bool {
        match parse_iso8601_utc(&self.expires_at) {
            Some(expires) => SystemTime::now() < expires,
            None => false,
        }
    }
}

pub fn load_stored_session() -> Result<Option<StoredSession>, CliError> {
    CompositeTokenStore::default_store()?.load()
}

pub fn load_active_session() -> Result<Option<StoredSession>, CliError> {
    Ok(load_stored_session()?.filter(|session| session.is_active()))
}

pub trait TokenStore {
    fn load(&self) -> Result<Option<StoredSession>, CliError>;
    fn save(&self, session: &StoredSession) -> Result<(), CliError>;
    fn clear(&self) -> Result<(), CliError>;
}

pub struct FileTokenStore {
    path: PathBuf,
}

impl FileTokenStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> Result<Self, CliError> {
        Ok(Self::new(default_credentials_path()?))
    }
}

impl TokenStore for FileTokenStore {
    fn load(&self) -> Result<Option<StoredSession>, CliError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&self.path)
            .map_err(|e| CliError::Other(format!("read credentials: {e}")))?;
        let session: StoredSession =
            toml::from_str(&raw).map_err(|e| CliError::Other(format!("parse credentials: {e}")))?;
        Ok(Some(session))
    }

    fn save(&self, session: &StoredSession) -> Result<(), CliError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| CliError::Other(format!("create config dir: {e}")))?;
        }
        let encoded = toml::to_string(session)
            .map_err(|e| CliError::Other(format!("encode credentials: {e}")))?;
        write_private_file(&self.path, encoded.as_bytes())
    }

    fn clear(&self) -> Result<(), CliError> {
        if self.path.exists() {
            fs::remove_file(&self.path)
                .map_err(|e| CliError::Other(format!("remove credentials: {e}")))?;
        }
        Ok(())
    }
}

pub struct KeyringTokenStore;

impl TokenStore for KeyringTokenStore {
    fn load(&self) -> Result<Option<StoredSession>, CliError> {
        let entry = keyring_entry()?;
        match entry.get_password() {
            Ok(raw) => {
                let session = serde_json::from_str(&raw)
                    .map_err(|e| CliError::Other(format!("parse keychain session: {e}")))?;
                Ok(Some(session))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(CliError::Other(format!("keychain read: {err}"))),
        }
    }

    fn save(&self, session: &StoredSession) -> Result<(), CliError> {
        let entry = keyring_entry()?;
        let raw = serde_json::to_string(session)
            .map_err(|e| CliError::Other(format!("encode keychain session: {e}")))?;
        entry
            .set_password(&raw)
            .map_err(|e| CliError::Other(format!("keychain write: {e}")))
    }

    fn clear(&self) -> Result<(), CliError> {
        let entry = keyring_entry()?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(CliError::Other(format!("keychain delete: {err}"))),
        }
    }
}

pub struct CompositeTokenStore {
    preferred: KeyringTokenStore,
    fallback: FileTokenStore,
}

impl CompositeTokenStore {
    pub fn default_store() -> Result<Self, CliError> {
        Ok(Self {
            preferred: KeyringTokenStore,
            fallback: FileTokenStore::default_path()?,
        })
    }
}

fn prefer_file_token_store() -> bool {
    matches!(
        std::env::var("DESLICER_TOKEN_STORE")
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("file")
    )
}

impl TokenStore for CompositeTokenStore {
    fn load(&self) -> Result<Option<StoredSession>, CliError> {
        if prefer_file_token_store() {
            return self.fallback.load();
        }
        match self.preferred.load() {
            Ok(Some(session)) => Ok(Some(session)),
            Ok(None) => self.fallback.load(),
            Err(_) => self.fallback.load(),
        }
    }

    fn save(&self, session: &StoredSession) -> Result<(), CliError> {
        if prefer_file_token_store() {
            return self.fallback.save(session);
        }
        match self.preferred.save(session) {
            Ok(()) => {
                let _ = self.fallback.clear();
                Ok(())
            }
            Err(_) => {
                eprintln!(
                    "warning: no OS keychain available; storing the CLI session in {} (0600)",
                    self.fallback.path.display()
                );
                self.fallback.save(session)
            }
        }
    }

    fn clear(&self) -> Result<(), CliError> {
        if prefer_file_token_store() {
            return self.fallback.clear();
        }
        let keyring = self.preferred.clear();
        let file = self.fallback.clear();
        keyring.or(file)
    }
}

pub fn default_credentials_path() -> Result<PathBuf, CliError> {
    if let Ok(dir) = std::env::var("DESLICER_CONFIG_DIR") {
        return Ok(PathBuf::from(dir).join("credentials.toml"));
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| CliError::Other("HOME is not set".into()))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("deslicer")
        .join("credentials.toml"))
}

fn keyring_entry() -> Result<keyring::Entry, CliError> {
    keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| CliError::Other(format!("keychain entry: {e}")))
}

/// DAI emits `Date.toISOString()` (`YYYY-MM-DDTHH:MM:SS.sssZ`).
fn parse_iso8601_utc(raw: &str) -> Option<SystemTime> {
    let body = raw.trim().strip_suffix('Z')?;
    let (date, clock) = body.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    let (hms, millis) = match clock.split_once('.') {
        Some((hms, frac)) => {
            let digits: String = frac.chars().take(3).collect();
            let ms: u64 = digits.parse().ok()?;
            (hms, ms)
        }
        None => (clock, 0),
    };
    let mut time_parts = hms.split(':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts.next()?.parse().ok()?;
    let days = days_from_civil(year, month, day)?;
    let secs = days
        .checked_mul(86_400)?
        .checked_add(i64::from(hour) * 3_600)?
        .checked_add(i64::from(minute) * 60)?
        .checked_add(i64::from(second))?;
    if secs < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(secs as u64) + Duration::from_millis(millis))
}

/// Howard Hinnant civil-to-days (proleptic Gregorian → Unix epoch days).
fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = i64::from(month) + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut opts = fs::OpenOptions::new();
    opts.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(path)
        .map_err(|e| CliError::Other(format!("write credentials: {e}")))?;
    file.write_all(bytes)
        .map_err(|e| CliError::Other(format!("write credentials: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_round_trips_a_session() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileTokenStore::new(dir.path().join("credentials.toml"));
        let session = StoredSession {
            cli_session_token: "dslcli_abc".into(),
            expires_at: "2026-08-14T20:00:00Z".into(),
            tenant_id: "tenant".into(),
            display_name: "Ada".into(),
            observer_api_url: "https://api.deslicer.ai/api/cli/observer/".into(),
            tenant_slug: None,
            deslicer_api_url: None,
        };
        store.save(&session).unwrap();
        assert_eq!(store.load().unwrap(), Some(session));
        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn iso8601_future_session_is_active() {
        let session = StoredSession {
            cli_session_token: "dslcli_abc".into(),
            expires_at: "2099-01-01T00:00:00.000Z".into(),
            tenant_id: "tenant".into(),
            display_name: "Ada".into(),
            observer_api_url: "https://api.deslicer.ai/api/cli/observer/".into(),
            tenant_slug: None,
            deslicer_api_url: None,
        };
        assert!(session.is_active());
    }

    #[test]
    fn iso8601_past_session_is_inactive() {
        let session = StoredSession {
            cli_session_token: "dslcli_abc".into(),
            expires_at: "2020-01-01T00:00:00Z".into(),
            tenant_id: "tenant".into(),
            display_name: "Ada".into(),
            observer_api_url: "https://api.deslicer.ai/api/cli/observer/".into(),
            tenant_slug: None,
            deslicer_api_url: None,
        };
        assert!(!session.is_active());
    }

    #[test]
    fn old_json_sessions_without_portal_url_still_parse() {
        let session: StoredSession = serde_json::from_str(
            r#"{
                "cli_session_token": "dslcli_abc",
                "expires_at": "2099-01-01T00:00:00Z",
                "tenant_id": "tenant",
                "display_name": "Ada",
                "observer_api_url": "https://ops.deslicer.show/api/cli/observer/"
            }"#,
        )
        .unwrap();
        assert!(session.deslicer_api_url.is_none());
        assert_eq!(
            session.observer_api_url,
            "https://ops.deslicer.show/api/cli/observer/"
        );
    }
}
