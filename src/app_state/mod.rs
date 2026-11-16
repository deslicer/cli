//! Local app-state fingerprint and file object store.
//!
//! This is the CLI-side contract for immutable Splunk app bytes. It is not a
//! RustFS client and it does not mint pre-signed DTS links.

pub mod fingerprint;
pub mod store;
