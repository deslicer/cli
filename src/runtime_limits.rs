//! Process-wide HTTP timeout and retry knobs from global CLI flags (#226).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 120;
const DEFAULT_RETRIES: u64 = 2;

static REQUEST_TIMEOUT_SECS: AtomicU64 = AtomicU64::new(DEFAULT_REQUEST_TIMEOUT_SECS);
static RETRIES: AtomicU64 = AtomicU64::new(DEFAULT_RETRIES);

pub fn set_from_cli(timeout_secs: Option<u64>, retries: Option<u64>) {
    if let Some(secs) = timeout_secs {
        REQUEST_TIMEOUT_SECS.store(secs.max(1), Ordering::Relaxed);
    }
    if let Some(n) = retries {
        RETRIES.store(n, Ordering::Relaxed);
    }
}

pub fn request_timeout() -> Duration {
    Duration::from_secs(REQUEST_TIMEOUT_SECS.load(Ordering::Relaxed).max(1))
}

/// Number of retries after the first attempt (`0` disables retry).
pub fn retries() -> u32 {
    RETRIES.load(Ordering::Relaxed).min(u64::from(u32::MAX)) as u32
}

pub fn max_attempts() -> u32 {
    retries().saturating_add(1).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_legacy_constants() {
        // Reset in case other tests mutated.
        set_from_cli(Some(DEFAULT_REQUEST_TIMEOUT_SECS), Some(DEFAULT_RETRIES));
        assert_eq!(request_timeout(), Duration::from_secs(120));
        assert_eq!(retries(), 2);
        assert_eq!(max_attempts(), 3);
    }

    #[test]
    fn zero_retries_means_single_attempt() {
        set_from_cli(None, Some(0));
        assert_eq!(retries(), 0);
        assert_eq!(max_attempts(), 1);
        set_from_cli(None, Some(DEFAULT_RETRIES));
    }
}
