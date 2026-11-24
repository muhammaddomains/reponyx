//! OS credential store access and PAT account flows (ADR-008,
//! ADR-004).
//!
//! Secrets live in the OS credential store only — Windows Credential
//! Manager, macOS Keychain, Linux Secret Service — never in SQLite,
//! JSON, logs, frontend state, or config. Short-lived installation
//! tokens are minted per use and discarded, never persisted here.
//! The GitHub App private key is never stored on the desktop at all
//! (Auth Broker only, ADR-010).

pub mod pat;
pub mod secret;
pub mod store;

pub use pat::{
    GITHUB_API_BASE, PAT_EXPIRY_WARNING_DAYS, PatExpiryStatus, PatInput, PatTokenSource,
    add_pat_account, delete_pat_account, pat_expiry_status, reconnect_pat_account, remove_pat,
    replace_pat, validate_pat_account,
};
pub use secret::SecretString;
pub use store::{
    ACCOUNT_DEVICE_TOKEN_PREFIX, ACCOUNT_SETUP_TOKEN, CredentialStore, MemoryCredentialStore,
    OsCredentialStore, SERVICE_BROKER, SERVICE_PAT, default_store, device_token_account,
    memory_store,
};
