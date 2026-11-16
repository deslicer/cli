//! Classify reqwest/transport failures so agents can tell DNS from a refused port.
//!
//! Exit 10 stays the transport class. `retryable` is false for mistakes that
//! will not heal (NXDOMAIN, invalid URL, connection refused, TLS) and true for
//! timeouts. See issue #280.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportCause {
    Dns,
    Refused,
    Tls,
    Timeout,
    InvalidUrl,
}

impl TransportCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dns => "dns",
            Self::Refused => "refused",
            Self::Tls => "tls",
            Self::Timeout => "timeout",
            Self::InvalidUrl => "invalid_url",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportDiagnosis {
    pub cause: TransportCause,
    pub url: Option<String>,
    pub retryable: bool,
    pub hint: &'static str,
}

pub fn diagnose(message: &str) -> Option<TransportDiagnosis> {
    let lower = message.to_ascii_lowercase();
    let cause = classify(&lower)?;
    Some(TransportDiagnosis {
        cause,
        url: extract_url(message),
        retryable: retryable(cause, &lower),
        hint: hint_for(cause),
    })
}

fn classify(lower: &str) -> Option<TransportCause> {
    if is_invalid_url(lower) {
        return Some(TransportCause::InvalidUrl);
    }
    if is_timeout(lower) {
        return Some(TransportCause::Timeout);
    }
    if is_tls(lower) {
        return Some(TransportCause::Tls);
    }
    if is_dns(lower) {
        return Some(TransportCause::Dns);
    }
    if is_refused(lower) {
        return Some(TransportCause::Refused);
    }
    None
}

fn retryable(cause: TransportCause, lower: &str) -> bool {
    match cause {
        TransportCause::Timeout => true,
        TransportCause::Dns => is_temporary_dns(lower),
        TransportCause::InvalidUrl | TransportCause::Refused | TransportCause::Tls => false,
    }
}

fn hint_for(cause: TransportCause) -> &'static str {
    match cause {
        TransportCause::Dns => {
            "DNS resolution failed. Check OBSERVER_API_URL / --observer-api-url."
        }
        TransportCause::Refused => {
            "Connection refused. Check OBSERVER_API_URL / --observer-api-url."
        }
        TransportCause::Tls => "TLS error. Check OBSERVER_API_URL / --observer-api-url.",
        TransportCause::Timeout => {
            "Request timed out. Check OBSERVER_API_URL / --observer-api-url."
        }
        TransportCause::InvalidUrl => "Invalid URL. Check OBSERVER_API_URL / --observer-api-url.",
    }
}

fn is_invalid_url(lower: &str) -> bool {
    lower.contains("invalid url")
        || lower.contains("unsupported url scheme")
        || lower.contains("plain http is only allowed")
        || lower.contains("relative url without a base")
        || lower.contains("not a valid url")
}

fn is_timeout(lower: &str) -> bool {
    lower.contains("timed out") || lower.contains("timeout") || lower.contains("deadline")
}

fn is_tls(lower: &str) -> bool {
    lower.contains("certificate")
        || lower.contains("tls")
        || lower.contains("ssl")
        || lower.contains("rustls")
        || lower.contains("unknownissuer")
}

fn is_dns(lower: &str) -> bool {
    lower.contains("dns")
        || lower.contains("name or service not known")
        || lower.contains("no such host")
        || lower.contains("nodename nor servname")
        || lower.contains("failed to lookup")
        || lower.contains("nxdomain")
}

fn is_temporary_dns(lower: &str) -> bool {
    lower.contains("temporary failure in name resolution") || lower.contains("servfail")
}

fn is_refused(lower: &str) -> bool {
    lower.contains("connection refused")
        || lower.contains("actively refused")
        || lower.contains("os error 111")
        || lower.contains("os error 61")
}

fn extract_url(message: &str) -> Option<String> {
    let marker = "for url (";
    let start = message.find(marker)? + marker.len();
    let rest = &message[start..];
    let end = rest.find(')')?;
    let url = rest[..end].trim();
    if url.is_empty() {
        None
    } else {
        Some(url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_nxdomain_is_not_retryable_and_names_the_url() {
        let message = "error sending request for url (https://nonexistent.invalid/api/v1/groups): \
             error trying to connect: dns error: failed to lookup address information: \
             Name or service not known";
        let diag = diagnose(message).expect("dns");
        assert_eq!(diag.cause, TransportCause::Dns);
        assert!(!diag.retryable);
        assert_eq!(
            diag.url.as_deref(),
            Some("https://nonexistent.invalid/api/v1/groups")
        );
        assert!(diag.hint.contains("DNS resolution failed"));
        assert!(diag.hint.contains("OBSERVER_API_URL"));
    }

    #[test]
    fn temporary_dns_stays_retryable() {
        let message = "error sending request for url (https://api.example/api/v1/groups): \
             dns error: temporary failure in name resolution";
        let diag = diagnose(message).expect("dns");
        assert_eq!(diag.cause, TransportCause::Dns);
        assert!(diag.retryable);
    }

    #[test]
    fn connection_refused_is_not_retryable() {
        let message = "error sending request for url (https://127.0.0.1:9/api/v1/groups): \
             tcp connect error: Connection refused (os error 111)";
        let diag = diagnose(message).expect("refused");
        assert_eq!(diag.cause, TransportCause::Refused);
        assert!(!diag.retryable);
        assert_eq!(
            diag.url.as_deref(),
            Some("https://127.0.0.1:9/api/v1/groups")
        );
    }

    #[test]
    fn tls_and_timeout_and_invalid_url() {
        let tls = diagnose("error sending request for url (https://wrong.example/): invalid peer certificate: UnknownIssuer")
            .expect("tls");
        assert_eq!(tls.cause, TransportCause::Tls);
        assert!(!tls.retryable);

        let timeout = diagnose(
            "error sending request for url (https://slow.example/api): operation timed out",
        )
        .expect("timeout");
        assert_eq!(timeout.cause, TransportCause::Timeout);
        assert!(timeout.retryable);

        let invalid = diagnose("invalid URL join: relative URL without a base").expect("url");
        assert_eq!(invalid.cause, TransportCause::InvalidUrl);
        assert!(!invalid.retryable);
    }

    #[test]
    fn json_decode_is_unclassified() {
        assert!(diagnose("invalid plan JSON: expected value").is_none());
    }
}
