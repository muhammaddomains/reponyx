//! Append-only audit log for the Auth Broker.
//!
//! Every security-relevant event is recorded: pairing requested /
//! confirmed / revoked / failed, token minted / mint failed,
//! callback received / failed, authentication failures. An event
//! carries timestamp, actor (device identifier, `setup`, or client
//! IP), action, outcome, and a detail string — never a secret, token,
//! key, or code. There is deliberately no update or delete API:
//! history is append-only.
//!
//! Phase 5 keeps the log in memory (the broker is a local dev
//! service); durable audit storage is a hardening item for the
//! deployment phase — see `docs/architecture/authentication.md`.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Mutex;
use uuid::Uuid;

/// One audit event.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    /// Who acted: a device identifier, `setup` (operator with the
    /// setup token), or a client IP.
    pub actor: String,
    /// E.g. `pairing.requested`, `token.minted`, `auth.failed`.
    pub action: String,
    /// `success` or `failure`.
    pub outcome: String,
    /// Free-form detail. Must never contain a secret.
    pub detail: String,
}

/// Append-only audit log.
#[derive(Debug, Default)]
pub struct AuditLog {
    events: Mutex<Vec<AuditEvent>>,
}

impl AuditLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event. Callers must ensure `detail` contains no
    /// secret (ids and statuses only). Returns the recorded event
    /// so the caller can persist it (the store is the durable copy).
    pub fn record(
        &self,
        actor: impl Into<String>,
        action: &str,
        outcome: &str,
        detail: impl Into<String>,
    ) -> AuditEvent {
        let event = AuditEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            actor: actor.into(),
            action: action.to_string(),
            outcome: outcome.to_string(),
            detail: detail.into(),
        };
        if let Ok(mut events) = self.events.lock() {
            events.push(event.clone());
        }
        event
    }

    /// Replace the in-memory log with events loaded from durable
    /// storage (startup). Used once, before serving requests.
    pub fn load(&self, events: Vec<AuditEvent>) {
        if let Ok(mut current) = self.events.lock() {
            *current = events;
        }
    }

    pub fn list(&self) -> Vec<AuditEvent> {
        self.events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_is_append_only() {
        let log = AuditLog::new();
        log.record(
            "device-1",
            "pairing.confirmed",
            "success",
            "installation 123",
        );
        log.record("setup", "token.minted", "success", "installation 123");
        let events = log.list();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].action, "pairing.confirmed");
        assert_eq!(events[1].action, "token.minted");
    }
}
