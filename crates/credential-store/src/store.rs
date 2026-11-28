//! The `CredentialStore` trait and its backends (ADR-008).
//!
//! Secrets live in the OS credential store only — Windows Credential
//! Manager on Windows, macOS Keychain, Linux Secret Service — never
//! in SQLite, JSON, logs, frontend state, or config. The trait is
//! keyed by `(service, account)`; callers use the well-known service
//! names in this module:
//!
//! - `SERVICE_PAT` (`reponyx:pat`) with account = the account's UUID:
//!   one fine-grained PAT per account.
//! - `SERVICE_BROKER` (`reponyx:broker`) with account
//!   `ACCOUNT_SETUP_TOKEN` (the broker setup token, entered once by
//!   the operator) or `device-token:<device_identifier>` (the device
//!   token issued when a pairing is confirmed).
//! - `SERVICE_DEVICE` (`reponyx:device`) with account
//!   `ACCOUNT_DEVICE_IDENTIFIER`: the stable per-machine device
//!   identifier (an identifier, not a secret — the OS store is
//!   just the storage that survives a deleted database).
//!
//! Listing entries is not something the OS stores support: the
//! desktop lists accounts from the database (the source of truth for
//! which accounts exist) and asks the store only for one account's
//! secret at a time. `list_accounts` therefore exists on the trait
//! for backends that can enumerate (the in-memory backend), and the
//! OS backend reports it as unsupported instead of silently
//! returning an empty list.
//!
//! Short-lived credentials (installation tokens) are minted per use
//! and discarded — they are never persisted here.

use crate::SecretString;
use reponyx_core::{ErrorCode, RepoNyxError};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

/// Service name for fine-grained PATs. Account: the account UUID.
pub const SERVICE_PAT: &str = "reponyx:pat";
/// Service name for Auth Broker credentials. Account: see below.
pub const SERVICE_BROKER: &str = "reponyx:broker";
/// Service name for the web-platform login session. Account: see below.
pub const SERVICE_WEB: &str = "reponyx:web";
/// Service name for the stable per-machine device identity. Account:
/// see below. The identifier is an identifier, not a secret — but
/// the OS store is the only per-machine storage that survives a
/// deleted database. The settings table cannot be the source of
/// truth, or every fresh database mints a new identity and the web
/// counts one machine as many devices (2026-11-28: three device
/// rows for one machine after two database deletes).
pub const SERVICE_DEVICE: &str = "reponyx:device";
/// Account name for the device identifier under [`SERVICE_DEVICE`].
pub const ACCOUNT_DEVICE_IDENTIFIER: &str = "identifier";
/// Account name for the web session token under [`SERVICE_WEB`]:
/// the bearer token the web issued at login (30-day TTL), stored
/// here so the desktop never keeps it in SQLite, frontend state,
/// or logs. Deleted on logout and when the web answers 401.
pub const ACCOUNT_WEB_SESSION: &str = "session";
/// Account name for the broker setup token under [`SERVICE_BROKER`].
pub const ACCOUNT_SETUP_TOKEN: &str = "setup-token";
/// Account-name prefix for a device token under [`SERVICE_BROKER`]:
/// `device-token:<device_identifier>`.
pub const ACCOUNT_DEVICE_TOKEN_PREFIX: &str = "device-token:";

/// Account name for a device token for `device_identifier`.
pub fn device_token_account(device_identifier: &str) -> String {
    format!("{ACCOUNT_DEVICE_TOKEN_PREFIX}{device_identifier}")
}

/// A secret store: put a secret in, get it back, delete it.
///
/// Implementations must never log a secret value and must map their
/// errors onto [`ErrorCode::CredentialStoreError`] (or
/// `AuthenticationExpired` when the stored grant is known gone).
/// All methods are synchronous: the OS stores are synchronous APIs,
/// and the PAT flow calls them from async code without blocking the
/// executor for long.
pub trait CredentialStore: Send + Sync {
    /// Store (or overwrite) the secret for `(service, account)`.
    fn store(&self, service: &str, account: &str, secret: &str) -> Result<(), RepoNyxError>;

    /// Retrieve the secret for `(service, account)`, or `None` when
    /// no entry exists. A store failure is an error, never `None`.
    fn get(&self, service: &str, account: &str) -> Result<Option<SecretString>, RepoNyxError>;

    /// Delete the entry for `(service, account)`. Deleting a missing
    /// entry is a no-op (returns `Ok`).
    fn delete(&self, service: &str, account: &str) -> Result<(), RepoNyxError>;

    /// `true` when an entry exists for `(service, account)`.
    fn exists(&self, service: &str, account: &str) -> Result<bool, RepoNyxError> {
        Ok(self.get(service, account)?.is_some())
    }

    /// Account names stored under `service`. Only backends that can
    /// enumerate support this; the OS backend returns
    /// `CredentialStoreError` — callers list accounts from the
    /// database instead.
    fn list_accounts(&self, service: &str) -> Result<Vec<String>, RepoNyxError> {
        let _ = service;
        Err(RepoNyxError::new(
            ErrorCode::CredentialStoreError,
            "this credential store does not support listing entries",
        ))
    }
}

fn store_error(message: impl Into<String>) -> RepoNyxError {
    RepoNyxError::new(ErrorCode::CredentialStoreError, message)
}

/// The OS credential store via the `keyring` crate: Windows
/// Credential Manager on Windows, macOS Keychain, Linux Secret
/// Service (D-Bus). One `keyring::Entry` per `(service, account)`.
pub struct OsCredentialStore;

impl OsCredentialStore {
    pub fn new() -> Self {
        Self
    }

    /// Probe whether the OS store answers at all, without writing
    /// anything: reading a missing entry must fail with
    /// `NoEntry` (the store responded) rather than a platform
    /// failure (no Secret Service / no Keychain available). Used by
    /// tests and by the desktop to explain an unavailable store.
    pub fn is_available() -> bool {
        let entry = keyring::Entry::new("reponyx:probe", "probe");
        match entry {
            Ok(entry) => match entry.get_password() {
                Ok(_) => true,
                Err(keyring::Error::NoEntry) => true,
                Err(_) => false,
            },
            Err(_) => false,
        }
    }
}

impl Default for OsCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for OsCredentialStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OsCredentialStore").finish()
    }
}

impl CredentialStore for OsCredentialStore {
    fn store(&self, service: &str, account: &str, secret: &str) -> Result<(), RepoNyxError> {
        let entry = keyring::Entry::new(service, account).map_err(|e| {
            store_error("cannot open the OS credential store").with_details(e.to_string())
        })?;
        // `update_password` overwrites; `set_password` creates. Try
        // update first so an existing entry is replaced, not duplicated.
        // `set_password` creates or updates the entry (keyring 4
        // has no separate update call).
        match entry.set_password(secret) {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => entry.set_password(secret).map_err(|e| {
                store_error("cannot store the credential in the OS credential store")
                    .with_details(e.to_string())
            }),
            Err(e) => Err(
                store_error("cannot store the credential in the OS credential store")
                    .with_details(e.to_string()),
            ),
        }
    }

    fn get(&self, service: &str, account: &str) -> Result<Option<SecretString>, RepoNyxError> {
        let entry = keyring::Entry::new(service, account).map_err(|e| {
            store_error("cannot open the OS credential store").with_details(e.to_string())
        })?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(SecretString::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(
                store_error("cannot read the credential from the OS credential store")
                    .with_details(e.to_string()),
            ),
        }
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), RepoNyxError> {
        let entry = keyring::Entry::new(service, account).map_err(|e| {
            store_error("cannot open the OS credential store").with_details(e.to_string())
        })?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(
                store_error("cannot delete the credential from the OS credential store")
                    .with_details(e.to_string()),
            ),
        }
    }
}

/// In-memory credential store: for tests and CI. Nothing is
/// persisted; secrets live only for the life of this value.
/// `Debug` never prints a secret.
pub struct MemoryCredentialStore {
    entries: RwLock<HashMap<(String, String), String>>,
}

impl MemoryCredentialStore {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MemoryCredentialStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self
            .entries
            .read()
            .map(|entries| entries.len())
            .unwrap_or(0);
        f.debug_struct("MemoryCredentialStore")
            .field("entries", &count)
            .finish()
    }
}

impl CredentialStore for MemoryCredentialStore {
    fn store(&self, service: &str, account: &str, secret: &str) -> Result<(), RepoNyxError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| store_error("in-memory credential store is poisoned"))?;
        entries.insert(
            (service.to_string(), account.to_string()),
            secret.to_string(),
        );
        Ok(())
    }

    fn get(&self, service: &str, account: &str) -> Result<Option<SecretString>, RepoNyxError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| store_error("in-memory credential store is poisoned"))?;
        Ok(entries
            .get(&(service.to_string(), account.to_string()))
            .map(|secret| SecretString::new(secret.clone())))
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), RepoNyxError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| store_error("in-memory credential store is poisoned"))?;
        entries.remove(&(service.to_string(), account.to_string()));
        Ok(())
    }

    fn list_accounts(&self, service: &str) -> Result<Vec<String>, RepoNyxError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| store_error("in-memory credential store is poisoned"))?;
        let mut accounts: Vec<String> = entries
            .keys()
            .filter(|(entry_service, _)| entry_service == service)
            .map(|(_, account)| account.clone())
            .collect();
        accounts.sort();
        Ok(accounts)
    }
}

/// The store the desktop uses: the OS credential store.
pub fn default_store() -> Arc<dyn CredentialStore> {
    Arc::new(OsCredentialStore::new())
}

/// The store tests use: in-memory, nothing persisted.
pub fn memory_store() -> Arc<dyn CredentialStore> {
    Arc::new(MemoryCredentialStore::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips_and_lists() {
        let store = MemoryCredentialStore::new();
        assert_eq!(store.get(SERVICE_PAT, "acc-1").unwrap(), None);
        assert!(!store.exists(SERVICE_PAT, "acc-1").unwrap());
        store
            .store(SERVICE_PAT, "acc-1", "test-pat-secret-value")
            .unwrap();
        assert!(store.exists(SERVICE_PAT, "acc-1").unwrap());
        assert_eq!(
            store.get(SERVICE_PAT, "acc-1").unwrap().unwrap().expose(),
            "test-pat-secret-value"
        );
        store.store(SERVICE_PAT, "acc-2", "test-pat-other").unwrap();
        assert_eq!(
            store.list_accounts(SERVICE_PAT).unwrap(),
            vec!["acc-1".to_string(), "acc-2".to_string()]
        );
        store
            .store(SERVICE_PAT, "acc-1", "test-pat-replaced")
            .unwrap();
        assert_eq!(
            store.get(SERVICE_PAT, "acc-1").unwrap().unwrap().expose(),
            "test-pat-replaced"
        );
        store.delete(SERVICE_PAT, "acc-1").unwrap();
        assert_eq!(store.get(SERVICE_PAT, "acc-1").unwrap(), None);
        // Deleting a missing entry is a no-op.
        store.delete(SERVICE_PAT, "acc-1").unwrap();
        // Secrets never appear in Debug output.
        store
            .store(SERVICE_PAT, "acc-2", "test-pat-do-not-leak")
            .unwrap();
        let debug = format!("{store:?}");
        assert!(!debug.contains("test-pat-do-not-leak"));
    }

    #[test]
    fn os_store_reports_listing_as_unsupported() {
        let store = OsCredentialStore::new();
        let err = store.list_accounts(SERVICE_PAT).unwrap_err();
        assert_eq!(err.code, ErrorCode::CredentialStoreError);
    }

    #[test]
    fn device_token_account_name_is_namespaced() {
        assert_eq!(device_token_account("machine-1"), "device-token:machine-1");
    }
}
