//! IPC shared secret stored in the OS credential store (never in SQLite).

use rand::RngExt;
use reponyx_core::{ErrorCode, RepoNyxError};
use reponyx_credential_store::{CredentialStore, SecretString};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use thiserror::Error;

/// Service name for the desktop↔agent shared secret.
pub const SERVICE_IPC: &str = "reponyx:ipc";
/// Account name under [`SERVICE_IPC`].
pub const ACCOUNT_SHARED_SECRET: &str = "shared-secret";

#[derive(Debug, Error)]
pub enum IpcAuthError {
    #[error("{0}")]
    Store(String),
    #[error("ipc shared secret missing; start the agent once to create it")]
    Missing,
}

impl From<IpcAuthError> for RepoNyxError {
    fn from(value: IpcAuthError) -> Self {
        match value {
            IpcAuthError::Missing => {
                RepoNyxError::new(ErrorCode::CredentialStoreError, value.to_string())
            }
            IpcAuthError::Store(msg) => RepoNyxError::new(ErrorCode::CredentialStoreError, msg),
        }
    }
}

pub fn load_shared_secret(store: &Arc<dyn CredentialStore>) -> Result<SecretString, IpcAuthError> {
    match store
        .get(SERVICE_IPC, ACCOUNT_SHARED_SECRET)
        .map_err(|e| IpcAuthError::Store(e.to_string()))?
    {
        Some(s) if !s.expose().is_empty() => Ok(s),
        _ => Err(IpcAuthError::Missing),
    }
}

pub fn store_shared_secret(
    store: &Arc<dyn CredentialStore>,
    secret: &str,
) -> Result<(), IpcAuthError> {
    store
        .store(SERVICE_IPC, ACCOUNT_SHARED_SECRET, secret)
        .map_err(|e| IpcAuthError::Store(e.to_string()))
}

/// Load the existing secret, or generate and persist a new 32-byte hex secret.
pub fn ensure_shared_secret(
    store: &Arc<dyn CredentialStore>,
) -> Result<SecretString, IpcAuthError> {
    match load_shared_secret(store) {
        Ok(s) => Ok(s),
        Err(IpcAuthError::Missing) => {
            let mut bytes = [0u8; 32];
            rand::rng().fill(&mut bytes);
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            store_shared_secret(store, &hex)?;
            Ok(SecretString::new(hex))
        }
        Err(e) => Err(e),
    }
}

/// Constant-time-ish compare of the presented token to the stored secret.
/// Uses SHA-256 digests so the comparison is always fixed-length.
pub fn token_matches(presented: &str, expected: &str) -> bool {
    let a = Sha256::digest(presented.as_bytes());
    let b = Sha256::digest(expected.as_bytes());
    a.as_slice() == b.as_slice()
}
