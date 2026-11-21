//! Resolve a per-account [`CredentialSource`] for Git operations.
//!
//! Phase 5 rules:
//! - Fine-grained PAT → read from OS credential store (`reponyx:pat`)
//! - GitHub App → mint installation token via Auth Broker (device token
//!   in store + installation id from the database)
//! - Tokens are never logged or written to SQLite

use async_trait::async_trait;
use reponyx_core::{
    Account, AccountStatus, AuthenticationMethod, ErrorCode, RepoNyxError,
};
use reponyx_credential_store::{
    ACCOUNT_DEVICE_TOKEN_PREFIX, CredentialStore, PatTokenSource, SERVICE_BROKER, SERVICE_PAT,
    device_token_account,
};
use reponyx_database::installations::list_installations_for_account;
use reponyx_github_provider::{BrokerTokenSource, CredentialSource};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// Builds a [`CredentialSource`] for one account.
#[async_trait]
pub trait CredentialResolver: Send + Sync + 'static {
    async fn resolve(
        &self,
        pool: &SqlitePool,
        account: &Account,
    ) -> Result<Arc<dyn CredentialSource>, RepoNyxError>;
}

/// Production resolver: PAT from the OS store; GitHub App via broker.
pub struct DefaultCredentialResolver {
    store: Arc<dyn CredentialStore>,
    /// Auth Broker base URL (default `http://127.0.0.1:8765`).
    broker_base_url: String,
    /// Desktop device identifier used to look up `device-token:<id>`.
    /// Required for GitHub App accounts; optional for PAT-only setups.
    device_identifier: Option<String>,
}

impl DefaultCredentialResolver {
    pub fn new(
        store: Arc<dyn CredentialStore>,
        broker_base_url: impl Into<String>,
        device_identifier: Option<String>,
    ) -> Self {
        Self {
            store,
            broker_base_url: broker_base_url.into(),
            device_identifier,
        }
    }

    /// PAT-only resolver (no broker). GitHub App accounts will fail with
    /// a clear error — useful for early desktop builds that only support PAT.
    pub fn pat_only(store: Arc<dyn CredentialStore>) -> Self {
        Self {
            store,
            broker_base_url: reponyx_github_provider::DEFAULT_BROKER_BASE_URL.to_string(),
            device_identifier: None,
        }
    }
}

#[async_trait]
impl CredentialResolver for DefaultCredentialResolver {
    async fn resolve(
        &self,
        pool: &SqlitePool,
        account: &Account,
    ) -> Result<Arc<dyn CredentialSource>, RepoNyxError> {
        match account.status {
            AccountStatus::Connected | AccountStatus::Paused => {}
            AccountStatus::TokenExpired => {
                return Err(RepoNyxError::new(
                    ErrorCode::AuthenticationExpired,
                    format!("account {} token expired; reconnect required", account.username),
                ));
            }
            AccountStatus::Revoked => {
                return Err(RepoNyxError::new(
                    ErrorCode::AuthenticationRevoked,
                    format!("account {} is revoked; reconnect required", account.username),
                ));
            }
            AccountStatus::Error => {
                return Err(RepoNyxError::new(
                    ErrorCode::CredentialStoreError,
                    format!("account {} is in error state", account.username),
                ));
            }
        }

        match account.authentication_method {
            AuthenticationMethod::FineGrainedPat => self.resolve_pat(account),
            AuthenticationMethod::GitHubApp => self.resolve_github_app(pool, account).await,
        }
    }
}

impl DefaultCredentialResolver {
    fn resolve_pat(&self, account: &Account) -> Result<Arc<dyn CredentialSource>, RepoNyxError> {
        // Verify the secret exists before handing a source to Git.
        let key = account.id.to_string();
        if self.store.get(SERVICE_PAT, &key)?.is_none() {
            return Err(RepoNyxError::new(
                ErrorCode::CredentialStoreError,
                "PAT not found in credential store; re-add the account",
            ));
        }
        Ok(Arc::new(PatTokenSource::new(
            Arc::clone(&self.store),
            account.id,
            account.username.clone(),
            account.pat_expires_at,
        )) as Arc<dyn CredentialSource>)
    }

    async fn resolve_github_app(
        &self,
        pool: &SqlitePool,
        account: &Account,
    ) -> Result<Arc<dyn CredentialSource>, RepoNyxError> {
        let device_id = self.device_identifier.as_deref().ok_or_else(|| {
            RepoNyxError::new(
                ErrorCode::InvalidInput,
                "GitHub App sync requires a paired device identifier",
            )
        })?;

        let device_token = self
            .store
            .get(SERVICE_BROKER, &device_token_account(device_id))?
            .ok_or_else(|| {
                RepoNyxError::new(
                    ErrorCode::CredentialStoreError,
                    "device token not found; re-pair with the Auth Broker",
                )
            })?;

        let installations = list_installations_for_account(pool, account.id).await?;
        let installation = installations
            .into_iter()
            .find(|i| i.installation_status == reponyx_core::InstallationStatus::Active)
            .ok_or_else(|| {
                RepoNyxError::new(
                    ErrorCode::InsufficientPermissions,
                    format!(
                        "no active GitHub App installation for account {}",
                        account.username
                    ),
                )
            })?;

        let source = BrokerTokenSource::new(
            &self.broker_base_url,
            installation.installation_id,
            device_token.expose(),
        )?;
        Ok(Arc::new(source) as Arc<dyn CredentialSource>)
    }
}

/// Test resolver that always returns the same static source.
pub struct StaticCredentialResolver {
    source: Arc<dyn CredentialSource>,
}

impl StaticCredentialResolver {
    pub fn new(source: Arc<dyn CredentialSource>) -> Arc<Self> {
        Arc::new(Self { source })
    }
}

#[async_trait]
impl CredentialResolver for StaticCredentialResolver {
    async fn resolve(
        &self,
        _pool: &SqlitePool,
        _account: &Account,
    ) -> Result<Arc<dyn CredentialSource>, RepoNyxError> {
        Ok(Arc::clone(&self.source))
    }
}

/// Look up a device identifier from settings if present.
/// Convention: setting key `device.identifier` (UUID string).
pub async fn device_identifier_from_settings(
    pool: &SqlitePool,
) -> Result<Option<String>, RepoNyxError> {
    match reponyx_database::settings::get_setting(pool, "device.identifier").await? {
        Some(s) if !s.value.trim().is_empty() => Ok(Some(s.value.trim().to_string())),
        _ => Ok(None),
    }
}

/// Storage root from settings (`storage.root`). Empty → error.
pub async fn storage_root_from_settings(
    pool: &SqlitePool,
) -> Result<std::path::PathBuf, RepoNyxError> {
    let setting = reponyx_database::settings::get_setting(pool, "storage.root")
        .await?
        .ok_or_else(|| {
            RepoNyxError::new(
                ErrorCode::InvalidInput,
                "storage.root is not configured",
            )
        })?;
    let path = setting.value.trim();
    if path.is_empty() {
        return Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            "storage.root is empty",
        ));
    }
    Ok(std::path::PathBuf::from(path))
}

/// Convenience: build a default resolver from a store + optional device id setting.
pub async fn default_resolver_from_settings(
    pool: &SqlitePool,
    store: Arc<dyn CredentialStore>,
    broker_base_url: &str,
) -> Result<Arc<DefaultCredentialResolver>, RepoNyxError> {
    let device = device_identifier_from_settings(pool).await?;
    Ok(Arc::new(DefaultCredentialResolver::new(
        store,
        broker_base_url,
        device,
    )))
}

// Silence unused import warning when PatTokenSource path differs across versions.
#[allow(dead_code)]
fn _pat_service_key(account_id: Uuid) -> String {
    account_id.to_string()
}

#[allow(dead_code)]
fn _broker_prefix() -> &'static str {
    ACCOUNT_DEVICE_TOKEN_PREFIX
}
