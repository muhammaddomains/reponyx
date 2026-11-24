//! Broker state: installations, pairing codes, `state` tokens,
//! device tokens, and the installation-token cache.
//!
//! Phase 10 (F1): state is durable. The working state lives in
//! memory maps (fast reads, the same behaviour as the in-memory-only
//! Phase 5 broker) and every mutation writes through to the SQLite
//! [`BrokerStore`] FIRST, then updates the map — so a failed
//! persist leaves the map untouched, and a restart reloads the maps
//! from the store. Only SHA-256 hashes of pairing codes, `state`
//! tokens, and device tokens are stored — never the raw values — so
//! a dump of broker state yields nothing an attacker can present.
//! Minted GitHub tokens are cached in memory only, until 5 minutes
//! before expiry (they are re-minted on demand after a restart);
//! the rate-limiter buckets are likewise in-memory only.
//!
//! The maps use `tokio::sync::RwLock` (not `std::sync`): mutations
//! hold a map's write lock across the single store write (one
//! SQLite statement, ~1 ms on a local file), which keeps the
//! decide-and-persist step atomic against concurrent requests for
//! the same code — and `std` guards are not `Send` across an await,
//! so they would make every handler's future non-`Send`. The store
//! never touches these locks, so there is no lock-ordering hazard.

use crate::audit::{AuditEvent, AuditLog};
use crate::auth::{normalize_pairing_code, pairing_code, random_token, sha256_hex};
use crate::config::BrokerConfig;
use crate::rate_limit::RateLimiter;
use crate::store::BrokerStore;
use crate::tokens::{InstallationTokenResponse, mint_installation_token};
use chrono::{DateTime, Utc};
use reponyx_core::{ErrorCode, RepoNyxError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A recorded GitHub App installation (from the install callback).
#[derive(Debug, Clone)]
pub struct InstallationRecord {
    pub installation_id: String,
    pub setup_action: String,
    pub account_login: Option<String>,
    pub installed_at: DateTime<Utc>,
    pub paired_device: Option<String>,
}

/// A pairing code, keyed by its SHA-256 hash. `expires_at` is
/// `None`: pairing codes never expire (Phase 10 fix — the old
/// 10-minute TTL expired while users were mid-flow). They stay
/// safe without it: single-use, bound to one device, confirm
/// requires the setup token, and wrong-device attempts revoke the
/// code after `max_pairing_attempts`.
#[derive(Debug, Clone)]
pub struct PairingCodeRecord {
    pub code_hash: String,
    pub installation_id: String,
    pub device_name: String,
    pub device_identifier: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    /// Wrong-device attempts so far; at `max_pairing_attempts` the
    /// code is revoked.
    pub attempts: u32,
    pub status: PairingStatus,
}

/// Pairing-code lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingStatus {
    Pending,
    Confirmed,
    Revoked,
}

impl PairingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PairingStatus::Pending => "pending",
            PairingStatus::Confirmed => "confirmed",
            PairingStatus::Revoked => "revoked",
        }
    }
}

/// A CSRF `state` value issued for the GitHub App install flow,
/// keyed by its SHA-256 hash.
#[derive(Debug, Clone)]
pub struct SetupStateRecord {
    pub state_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used: bool,
}

/// A device token issued after pairing, keyed by its SHA-256 hash.
/// Non-expiring by time in Phase 5; revocable.
#[derive(Debug, Clone)]
pub struct DeviceTokenRecord {
    pub token_hash: String,
    pub device_identifier: String,
    pub device_name: String,
    pub installation_id: String,
    pub created_at: DateTime<Utc>,
    pub revoked: bool,
}

/// A cached installation token (in memory only). `Debug` redacts
/// the token.
pub struct CachedInstallationToken {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for CachedInstallationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedInstallationToken")
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// What a pairing-status lookup reports.
#[derive(Debug, Clone)]
pub struct PairingStatusInfo {
    pub status: String,
    pub installation_id: Option<String>,
    pub device_identifier: Option<String>,
}

/// All broker state. Handlers take `Arc<BrokerState>`.
pub struct BrokerState {
    pub config: BrokerConfig,
    pub http: reqwest::Client,
    /// Durable backing store. `None` means pure in-memory mode
    /// (tests, and `BrokerState::new`); production uses
    /// [`BrokerState::open`].
    store: Option<Arc<BrokerStore>>,
    installations: RwLock<HashMap<String, InstallationRecord>>,
    pairing_codes: RwLock<HashMap<String, PairingCodeRecord>>,
    setup_states: RwLock<HashMap<String, SetupStateRecord>>,
    device_tokens: RwLock<HashMap<String, DeviceTokenRecord>>,
    token_cache: RwLock<HashMap<String, CachedInstallationToken>>,
    audit: AuditLog,
    pub rate_limiter: RateLimiter,
}

fn store_error(err: sqlx::Error) -> RepoNyxError {
    RepoNyxError::new(
        ErrorCode::InternalError,
        format!("broker store unavailable: {err}"),
    )
}

impl BrokerState {
    fn new_inner(config: BrokerConfig, store: Option<Arc<BrokerStore>>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("RepoNyx-Auth-Broker/0.1.0")
            .build()
            .expect("reqwest client builds with a static config");
        Self {
            config,
            http,
            store,
            installations: RwLock::new(HashMap::new()),
            pairing_codes: RwLock::new(HashMap::new()),
            setup_states: RwLock::new(HashMap::new()),
            device_tokens: RwLock::new(HashMap::new()),
            token_cache: RwLock::new(HashMap::new()),
            audit: AuditLog::new(),
            rate_limiter: RateLimiter::new(),
        }
    }

    pub fn new(config: BrokerConfig) -> Arc<Self> {
        Arc::new(Self::new_inner(config, None))
    }

    /// Open broker state backed by `store`: prune spent rows and
    /// stale audit events, then load every record into the memory
    /// maps so a restart resumes where the broker left off.
    pub async fn open(
        config: BrokerConfig,
        store: Arc<BrokerStore>,
    ) -> Result<Arc<Self>, sqlx::Error> {
        store.prune_spent_pairing_codes().await?;
        store.prune_setup_states().await?;
        store.prune_audit_events().await?;
        let installations = store.load_installations().await?;
        let pairing_codes = store.load_pairing_codes().await?;
        let setup_states = store.load_setup_states().await?;
        let device_tokens = store.load_device_tokens().await?;
        let audit_events = store.load_audit_events().await?;
        let state = Arc::new(Self::new_inner(config, Some(Arc::clone(&store))));
        {
            let mut installations_map = state.installations.write().await;
            for record in installations {
                installations_map.insert(record.installation_id.clone(), record);
            }
        }
        {
            let mut codes = state.pairing_codes.write().await;
            for record in pairing_codes {
                codes.insert(record.code_hash.clone(), record);
            }
        }
        {
            let mut states = state.setup_states.write().await;
            for record in setup_states {
                states.insert(record.state_hash.clone(), record);
            }
        }
        {
            let mut tokens = state.device_tokens.write().await;
            for record in device_tokens {
                tokens.insert(record.token_hash.clone(), record);
            }
        }
        state.audit.load(audit_events);
        Ok(state)
    }

    // -- audit -------------------------------------------------------

    /// Record an audit event: persist it first (best-effort — a
    /// store failure is logged, not fatal, because auditing must
    /// never break the request it describes), then append it to the
    /// in-memory log.
    pub async fn audit(
        &self,
        actor: impl Into<String>,
        action: &str,
        outcome: &str,
        detail: impl Into<String>,
    ) {
        let event = self.audit.record(actor, action, outcome, detail);
        if let Some(store) = &self.store
            && let Err(err) = store.insert_audit_event(&event).await
        {
            tracing::warn!(error = %err, "broker audit event could not be persisted");
        }
    }

    pub fn audit_events(&self) -> Vec<AuditEvent> {
        self.audit.list()
    }

    // -- installations ----------------------------------------------

    pub async fn record_installation(
        &self,
        installation_id: &str,
        setup_action: &str,
        account_login: Option<String>,
    ) -> Result<(), RepoNyxError> {
        let record = InstallationRecord {
            installation_id: installation_id.to_string(),
            setup_action: setup_action.to_string(),
            account_login,
            installed_at: Utc::now(),
            paired_device: None,
        };
        if let Some(store) = &self.store {
            store
                .upsert_installation(&record)
                .await
                .map_err(store_error)?;
        }
        self.installations
            .write()
            .await
            .insert(installation_id.to_string(), record);
        Ok(())
    }

    pub async fn installation(&self, installation_id: &str) -> Option<InstallationRecord> {
        self.installations
            .read()
            .await
            .get(installation_id)
            .cloned()
    }

    pub async fn list_installations(&self) -> Vec<InstallationRecord> {
        self.installations.read().await.values().cloned().collect()
    }

    // -- CSRF state --------------------------------------------------

    /// Issue a one-time `state` value for the GitHub App install
    /// flow. Returns the raw value (sent to GitHub) and its expiry.
    pub async fn issue_setup_state(&self) -> Result<(String, DateTime<Utc>), RepoNyxError> {
        let state = random_token();
        let expires_at = Utc::now() + self.config.state_ttl;
        let record = SetupStateRecord {
            state_hash: sha256_hex(&state),
            created_at: Utc::now(),
            expires_at,
            used: false,
        };
        if let Some(store) = &self.store {
            store.prune_setup_states().await.map_err(store_error)?;
            store
                .insert_setup_state(&record)
                .await
                .map_err(store_error)?;
        }
        self.setup_states
            .write()
            .await
            .insert(record.state_hash.clone(), record);
        Ok((state, expires_at))
    }

    /// Consume a `state` value: true only when it exists, is unused,
    /// and has not expired. Consumption is single-use, and the
    /// consumption survives a restart (the store is updated before
    /// the in-memory flag).
    pub async fn consume_setup_state(&self, state: &str) -> Result<bool, RepoNyxError> {
        let hash = sha256_hex(state);
        let mut states = self.setup_states.write().await;
        let Some(record) = states.get_mut(&hash) else {
            return Ok(false);
        };
        if record.used || Utc::now() > record.expires_at {
            return Ok(false);
        }
        if let Some(store) = &self.store {
            let mut updated = record.clone();
            updated.used = true;
            store
                .update_setup_state(&updated)
                .await
                .map_err(store_error)?;
        }
        record.used = true;
        Ok(true)
    }

    // -- pairing -----------------------------------------------------

    /// Request a pairing code for an installation. The installation
    /// must already be recorded (via the install callback).
    /// Returns the raw code (shown to the user) and `None` for its
    /// expiry — pairing codes never expire.
    pub async fn request_pairing(
        &self,
        installation_id: &str,
        device_name: &str,
        device_identifier: &str,
    ) -> Result<(String, Option<DateTime<Utc>>), RepoNyxError> {
        if self.installation(installation_id).await.is_none() {
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "installation not recorded; install the GitHub App first",
            ));
        }
        let code = pairing_code();
        let hash = sha256_hex(&normalize_pairing_code(&code));
        let record = PairingCodeRecord {
            code_hash: hash.clone(),
            installation_id: installation_id.to_string(),
            device_name: device_name.to_string(),
            device_identifier: device_identifier.to_string(),
            created_at: Utc::now(),
            expires_at: None,
            attempts: 0,
            status: PairingStatus::Pending,
        };
        if let Some(store) = &self.store {
            store
                .prune_spent_pairing_codes()
                .await
                .map_err(store_error)?;
            store
                .insert_pairing_code(&record)
                .await
                .map_err(store_error)?;
        }
        self.pairing_codes.write().await.insert(hash, record);
        Ok((code, None))
    }

    /// Confirm a pairing code. On success returns the device token
    /// (shown to the user once; only its hash is stored) and the
    /// installation id. Wrong device, expired, reused, or revoked
    /// codes are rejected, and wrong-device attempts count toward
    /// the per-code limit. Every mutation is persisted before the
    /// in-memory map changes; if a multi-step persist fails
    /// partway, the caller gets an error and a retry converges,
    /// because every step is idempotent (and a restart loads the
    /// persisted state).
    pub async fn confirm_pairing(
        &self,
        code: &str,
        device_identifier: &str,
    ) -> Result<(String, String), RepoNyxError> {
        let hash = sha256_hex(&normalize_pairing_code(code));
        let mut codes = self.pairing_codes.write().await;
        let Some(record) = codes.get_mut(&hash) else {
            drop(codes);
            self.audit(
                device_identifier,
                "pairing.failed",
                "failure",
                "unknown pairing code",
            )
            .await;
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "invalid pairing code",
            ));
        };
        if record.status == PairingStatus::Revoked {
            drop(codes);
            self.audit(
                device_identifier,
                "pairing.failed",
                "failure",
                "revoked pairing code",
            )
            .await;
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "pairing code was revoked",
            ));
        }
        if record.status == PairingStatus::Confirmed {
            drop(codes);
            self.audit(
                device_identifier,
                "pairing.failed",
                "failure",
                "pairing code already used",
            )
            .await;
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "pairing code already used",
            ));
        }
        if record.device_identifier != device_identifier {
            // Persist first, then apply to the map: a failed store
            // write must not consume an attempt.
            let mut updated = record.clone();
            updated.attempts += 1;
            let revoked = updated.attempts >= self.config.max_pairing_attempts;
            if revoked {
                updated.status = PairingStatus::Revoked;
            }
            if let Some(store) = &self.store {
                store
                    .update_pairing_code(&updated)
                    .await
                    .map_err(store_error)?;
            }
            *record = updated;
            drop(codes);
            if revoked {
                self.audit(
                    device_identifier,
                    "pairing.failed",
                    "failure",
                    "pairing code invalidated after too many wrong-device attempts",
                )
                .await;
                return Err(RepoNyxError::new(
                    ErrorCode::InvalidInput,
                    "pairing code invalidated after too many attempts",
                ));
            }
            self.audit(
                device_identifier,
                "pairing.failed",
                "failure",
                "pairing code bound to a different device",
            )
            .await;
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "pairing code is bound to a different device",
            ));
        }
        // Success: confirm the code, mint a device token, and mark
        // the installation paired — persisted before the maps change.
        let installation_id = record.installation_id.clone();
        let device_name = record.device_name.clone();
        let mut confirmed = record.clone();
        confirmed.status = PairingStatus::Confirmed;
        let device_token = random_token();
        let token_record = DeviceTokenRecord {
            token_hash: sha256_hex(&device_token),
            device_identifier: device_identifier.to_string(),
            device_name,
            installation_id: installation_id.clone(),
            created_at: Utc::now(),
            revoked: false,
        };
        if let Some(store) = &self.store {
            store
                .update_pairing_code(&confirmed)
                .await
                .map_err(store_error)?;
            store
                .insert_device_token(&token_record)
                .await
                .map_err(store_error)?;
            let mut installation = self.installation(&installation_id).await.ok_or_else(|| {
                RepoNyxError::new(
                    ErrorCode::InternalError,
                    "installation vanished during pairing",
                )
            })?;
            installation.paired_device = Some(device_identifier.to_string());
            store
                .upsert_installation(&installation)
                .await
                .map_err(store_error)?;
        }
        *record = confirmed;
        drop(codes);
        self.device_tokens
            .write()
            .await
            .insert(token_record.token_hash.clone(), token_record);
        if let Some(installation) = self.installations.write().await.get_mut(&installation_id) {
            installation.paired_device = Some(device_identifier.to_string());
        }
        self.audit(
            device_identifier,
            "pairing.confirmed",
            "success",
            format!("installation {installation_id}"),
        )
        .await;
        Ok((device_token, installation_id))
    }

    /// Revoke every device token and pending pairing code bound to a
    /// device. Returns true when anything was revoked. Persisted
    /// before the maps change; a retry after a partial store failure
    /// converges because revocation is idempotent.
    pub async fn revoke_device(&self, device_identifier: &str) -> Result<bool, RepoNyxError> {
        // Collect first (read-only), persist, then apply to the
        // maps — a failed store write must not revoke anything in
        // memory.
        let mut tokens_to_revoke: Vec<DeviceTokenRecord> = Vec::new();
        let mut codes_to_revoke: Vec<PairingCodeRecord> = Vec::new();
        {
            let tokens = self.device_tokens.read().await;
            for token in tokens.values() {
                if token.device_identifier == device_identifier && !token.revoked {
                    let mut updated = token.clone();
                    updated.revoked = true;
                    tokens_to_revoke.push(updated);
                }
            }
        }
        {
            let codes = self.pairing_codes.read().await;
            for code in codes.values() {
                if code.device_identifier == device_identifier
                    && code.status == PairingStatus::Pending
                {
                    let mut updated = code.clone();
                    updated.status = PairingStatus::Revoked;
                    codes_to_revoke.push(updated);
                }
            }
        }
        if tokens_to_revoke.is_empty() && codes_to_revoke.is_empty() {
            return Ok(false);
        }
        if let Some(store) = &self.store {
            for token in &tokens_to_revoke {
                store
                    .update_device_token(token)
                    .await
                    .map_err(store_error)?;
            }
            for code in &codes_to_revoke {
                store.update_pairing_code(code).await.map_err(store_error)?;
            }
        }
        {
            let mut tokens = self.device_tokens.write().await;
            for updated in &tokens_to_revoke {
                if let Some(token) = tokens.get_mut(&updated.token_hash) {
                    token.revoked = true;
                }
            }
        }
        {
            let mut codes = self.pairing_codes.write().await;
            for updated in &codes_to_revoke {
                if let Some(code) = codes.get_mut(&updated.code_hash) {
                    code.status = PairingStatus::Revoked;
                }
            }
        }
        self.audit(
            device_identifier,
            "pairing.revoked",
            "success",
            format!("device {device_identifier}"),
        )
        .await;
        Ok(true)
    }

    /// Status of a pairing code (for the desktop polling flow).
    pub async fn pairing_status(&self, code: &str) -> PairingStatusInfo {
        let hash = sha256_hex(&normalize_pairing_code(code));
        let Some(record) = self.pairing_codes.read().await.get(&hash).cloned() else {
            return PairingStatusInfo {
                status: "unknown".to_string(),
                installation_id: None,
                device_identifier: None,
            };
        };
        let status = record.status.as_str().to_string();
        PairingStatusInfo {
            status,
            installation_id: Some(record.installation_id),
            device_identifier: Some(record.device_identifier),
        }
    }

    /// Resolve a presented device token to its record (hash match,
    /// not revoked). Returns `None` for unknown or revoked tokens.
    pub async fn device_for_token(&self, token: &str) -> Option<DeviceTokenRecord> {
        let hash = sha256_hex(token);
        self.device_tokens
            .read()
            .await
            .get(&hash)
            .filter(|record| !record.revoked)
            .cloned()
    }

    // -- token minting -----------------------------------------------

    /// Mint (or return the cached) installation token for an
    /// installation. The cache is used until 5 minutes before
    /// expiry; `force_refresh` bypasses it (callers use this after a
    /// 401 with a cached token).
    pub async fn mint_token(
        &self,
        installation_id: &str,
        force_refresh: bool,
    ) -> Result<InstallationTokenResponse, RepoNyxError> {
        if !force_refresh {
            let cache = self.token_cache.read().await;
            if let Some(cached) = cache.get(installation_id)
                && Utc::now() + chrono::Duration::minutes(5) < cached.expires_at
            {
                return Ok(InstallationTokenResponse {
                    token: cached.token.clone(),
                    expires_at: cached.expires_at,
                    permissions: serde_json::Value::Object(Default::default()),
                    repository_selection: None,
                });
            }
        }
        let minted = mint_installation_token(&self.http, &self.config, installation_id).await?;
        self.token_cache.write().await.insert(
            installation_id.to_string(),
            CachedInstallationToken {
                token: minted.token.clone(),
                expires_at: minted.expires_at,
            },
        );
        Ok(minted)
    }
}
