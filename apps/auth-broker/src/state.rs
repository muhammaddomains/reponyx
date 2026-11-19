//! Broker state: installations, pairing codes, `state` tokens,
//! device tokens, and the installation-token cache.
//!
//! Phase 5 keeps state in memory (the broker is a local dev
//! service); durable broker storage is a hardening item for the
//! deployment phase. Only SHA-256 hashes of pairing codes, `state`
//! tokens, and device tokens are stored — never the raw values — so
//! a dump of broker state yields nothing an attacker can present.
//! Minted GitHub tokens are cached in memory only, until 5 minutes
//! before expiry.

use crate::audit::{AuditEvent, AuditLog};
use crate::auth::{normalize_pairing_code, pairing_code, random_token, sha256_hex};
use crate::config::BrokerConfig;
use crate::rate_limit::RateLimiter;
use crate::tokens::{InstallationTokenResponse, mint_installation_token};
use chrono::{DateTime, Utc};
use reponyx_core::{ErrorCode, RepoNyxError};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A recorded GitHub App installation (from the install callback).
#[derive(Debug, Clone)]
pub struct InstallationRecord {
    pub installation_id: String,
    pub setup_action: String,
    pub account_login: Option<String>,
    pub installed_at: DateTime<Utc>,
    pub paired_device: Option<String>,
}

/// A pairing code, keyed by its SHA-256 hash.
#[derive(Debug, Clone)]
pub struct PairingCodeRecord {
    pub code_hash: String,
    pub installation_id: String,
    pub device_name: String,
    pub device_identifier: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Wrong-device attempts so far; at `max_pairing_attempts` the
    /// code is revoked.
    pub attempts: u32,
    pub status: PairingStatus,
}

/// Pairing-code lifecycle. Expiry is computed from `expires_at`,
/// not stored.
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
    installations: RwLock<HashMap<String, InstallationRecord>>,
    pairing_codes: RwLock<HashMap<String, PairingCodeRecord>>,
    setup_states: RwLock<HashMap<String, SetupStateRecord>>,
    device_tokens: RwLock<HashMap<String, DeviceTokenRecord>>,
    token_cache: RwLock<HashMap<String, CachedInstallationToken>>,
    audit: AuditLog,
    pub rate_limiter: RateLimiter,
}

impl BrokerState {
    pub fn new(config: BrokerConfig) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("RepoNyx-Auth-Broker/0.1.0")
            .build()
            .expect("reqwest client builds with a static config");
        Arc::new(Self {
            config,
            http,
            installations: RwLock::new(HashMap::new()),
            pairing_codes: RwLock::new(HashMap::new()),
            setup_states: RwLock::new(HashMap::new()),
            device_tokens: RwLock::new(HashMap::new()),
            token_cache: RwLock::new(HashMap::new()),
            audit: AuditLog::new(),
            rate_limiter: RateLimiter::new(),
        })
    }

    // -- audit -------------------------------------------------------

    pub fn audit(
        &self,
        actor: impl Into<String>,
        action: &str,
        outcome: &str,
        detail: impl Into<String>,
    ) {
        self.audit.record(actor, action, outcome, detail);
    }

    pub fn audit_events(&self) -> Vec<AuditEvent> {
        self.audit.list()
    }

    // -- installations ----------------------------------------------

    pub fn record_installation(
        &self,
        installation_id: &str,
        setup_action: &str,
        account_login: Option<String>,
    ) {
        if let Ok(mut installations) = self.installations.write() {
            installations.insert(
                installation_id.to_string(),
                InstallationRecord {
                    installation_id: installation_id.to_string(),
                    setup_action: setup_action.to_string(),
                    account_login,
                    installed_at: Utc::now(),
                    paired_device: None,
                },
            );
        }
    }

    pub fn installation(&self, installation_id: &str) -> Option<InstallationRecord> {
        self.installations
            .read()
            .ok()?
            .get(installation_id)
            .cloned()
    }

    pub fn list_installations(&self) -> Vec<InstallationRecord> {
        self.installations
            .read()
            .map(|installations| installations.values().cloned().collect())
            .unwrap_or_default()
    }

    // -- CSRF state --------------------------------------------------

    /// Issue a one-time `state` value for the GitHub App install
    /// flow. Returns the raw value (sent to GitHub) and its expiry.
    pub fn issue_setup_state(&self) -> (String, DateTime<Utc>) {
        let state = random_token();
        let expires_at = Utc::now() + self.config.state_ttl;
        if let Ok(mut states) = self.setup_states.write() {
            states.insert(
                sha256_hex(&state),
                SetupStateRecord {
                    state_hash: sha256_hex(&state),
                    created_at: Utc::now(),
                    expires_at,
                    used: false,
                },
            );
        }
        (state, expires_at)
    }

    /// Consume a `state` value: true only when it exists, is unused,
    /// and has not expired. Consumption is single-use.
    pub fn consume_setup_state(&self, state: &str) -> bool {
        let hash = sha256_hex(state);
        let Ok(mut states) = self.setup_states.write() else {
            return false;
        };
        let Some(record) = states.get_mut(&hash) else {
            return false;
        };
        if record.used || Utc::now() > record.expires_at {
            return false;
        }
        record.used = true;
        true
    }

    // -- pairing -----------------------------------------------------

    /// Request a pairing code for an installation. The installation
    /// must already be recorded (via the install callback).
    /// Returns the raw code (shown to the user) and its expiry.
    pub fn request_pairing(
        &self,
        installation_id: &str,
        device_name: &str,
        device_identifier: &str,
    ) -> Result<(String, DateTime<Utc>), RepoNyxError> {
        if self.installation(installation_id).is_none() {
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "installation not recorded; install the GitHub App first",
            ));
        }
        let code = pairing_code();
        let expires_at = Utc::now() + self.config.pairing_ttl;
        let hash = sha256_hex(&normalize_pairing_code(&code));
        if let Ok(mut codes) = self.pairing_codes.write() {
            codes.insert(
                hash.clone(),
                PairingCodeRecord {
                    code_hash: hash,
                    installation_id: installation_id.to_string(),
                    device_name: device_name.to_string(),
                    device_identifier: device_identifier.to_string(),
                    created_at: Utc::now(),
                    expires_at,
                    attempts: 0,
                    status: PairingStatus::Pending,
                },
            );
        }
        Ok((code, expires_at))
    }

    /// Confirm a pairing code. On success returns the device token
    /// (shown to the user once; only its hash is stored) and the
    /// installation id. Wrong device, expired, reused, or revoked
    /// codes are rejected, and wrong-device attempts count toward
    /// the per-code limit.
    pub fn confirm_pairing(
        &self,
        code: &str,
        device_identifier: &str,
    ) -> Result<(String, String), RepoNyxError> {
        let hash = sha256_hex(&normalize_pairing_code(code));
        let mut codes = self.pairing_codes.write().map_err(|_| {
            RepoNyxError::new(ErrorCode::InternalError, "pairing store unavailable")
        })?;
        let Some(record) = codes.get_mut(&hash) else {
            self.audit(
                device_identifier,
                "pairing.failed",
                "failure",
                "unknown pairing code",
            );
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "invalid pairing code",
            ));
        };
        if record.status == PairingStatus::Revoked {
            self.audit(
                device_identifier,
                "pairing.failed",
                "failure",
                "revoked pairing code",
            );
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "pairing code was revoked",
            ));
        }
        if record.status == PairingStatus::Confirmed {
            self.audit(
                device_identifier,
                "pairing.failed",
                "failure",
                "pairing code already used",
            );
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "pairing code already used",
            ));
        }
        if Utc::now() > record.expires_at {
            self.audit(
                device_identifier,
                "pairing.failed",
                "failure",
                "expired pairing code",
            );
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "pairing code expired",
            ));
        }
        if record.device_identifier != device_identifier {
            record.attempts += 1;
            if record.attempts >= self.config.max_pairing_attempts {
                record.status = PairingStatus::Revoked;
                self.audit(
                    device_identifier,
                    "pairing.failed",
                    "failure",
                    "pairing code invalidated after too many wrong-device attempts",
                );
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
            );
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "pairing code is bound to a different device",
            ));
        }
        record.status = PairingStatus::Confirmed;
        let installation_id = record.installation_id.clone();
        let device_name = record.device_name.clone();
        drop(codes);

        let device_token = random_token();
        if let Ok(mut tokens) = self.device_tokens.write() {
            tokens.insert(
                sha256_hex(&device_token),
                DeviceTokenRecord {
                    token_hash: sha256_hex(&device_token),
                    device_identifier: device_identifier.to_string(),
                    device_name,
                    installation_id: installation_id.clone(),
                    created_at: Utc::now(),
                    revoked: false,
                },
            );
        }
        if let Ok(mut installations) = self.installations.write()
            && let Some(installation) = installations.get_mut(&installation_id)
        {
            installation.paired_device = Some(device_identifier.to_string());
        }
        self.audit(
            device_identifier,
            "pairing.confirmed",
            "success",
            format!("installation {installation_id}"),
        );
        Ok((device_token, installation_id))
    }

    /// Revoke every device token and pending pairing code bound to a
    /// device. Returns true when anything was revoked.
    pub fn revoke_device(&self, device_identifier: &str) -> bool {
        let mut revoked_any = false;
        if let Ok(mut tokens) = self.device_tokens.write() {
            for token in tokens.values_mut() {
                if token.device_identifier == device_identifier && !token.revoked {
                    token.revoked = true;
                    revoked_any = true;
                }
            }
        }
        if let Ok(mut codes) = self.pairing_codes.write() {
            for code in codes.values_mut() {
                if code.device_identifier == device_identifier
                    && code.status == PairingStatus::Pending
                {
                    code.status = PairingStatus::Revoked;
                    revoked_any = true;
                }
            }
        }
        if revoked_any {
            self.audit(
                device_identifier,
                "pairing.revoked",
                "success",
                format!("device {device_identifier}"),
            );
        }
        revoked_any
    }

    /// Status of a pairing code (for the desktop polling flow).
    pub fn pairing_status(&self, code: &str) -> PairingStatusInfo {
        let hash = sha256_hex(&normalize_pairing_code(code));
        let Some(record) = self
            .pairing_codes
            .read()
            .ok()
            .and_then(|codes| codes.get(&hash).cloned())
        else {
            return PairingStatusInfo {
                status: "unknown".to_string(),
                installation_id: None,
                device_identifier: None,
            };
        };
        let status = if record.status == PairingStatus::Pending && Utc::now() > record.expires_at {
            "expired".to_string()
        } else {
            record.status.as_str().to_string()
        };
        PairingStatusInfo {
            status,
            installation_id: Some(record.installation_id),
            device_identifier: Some(record.device_identifier),
        }
    }

    /// Resolve a presented device token to its record (hash match,
    /// not revoked). Returns `None` for unknown or revoked tokens.
    pub fn device_for_token(&self, token: &str) -> Option<DeviceTokenRecord> {
        let hash = sha256_hex(token);
        self.device_tokens
            .read()
            .ok()?
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
        if !force_refresh
            && let Ok(cache) = self.token_cache.read()
            && let Some(cached) = cache.get(installation_id)
            && Utc::now() + chrono::Duration::minutes(5) < cached.expires_at
        {
            return Ok(InstallationTokenResponse {
                token: cached.token.clone(),
                expires_at: cached.expires_at,
                permissions: serde_json::Value::Object(Default::default()),
                repository_selection: None,
            });
        }
        let minted = mint_installation_token(&self.http, &self.config, installation_id).await?;
        if let Ok(mut cache) = self.token_cache.write() {
            cache.insert(
                installation_id.to_string(),
                CachedInstallationToken {
                    token: minted.token.clone(),
                    expires_at: minted.expires_at,
                },
            );
        }
        Ok(minted)
    }
}
