//! Versioned message types for agent ↔ desktop IPC.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Current wire protocol version. Unknown versions are rejected.
pub const PROTOCOL_VERSION: u32 = 1;

/// Desktop → agent requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Ping {
        protocol_version: u32,
        /// Shared secret established at install / first agent start.
        token: String,
        request_id: Uuid,
    },
    AgentStatus {
        protocol_version: u32,
        token: String,
        request_id: Uuid,
    },
    /// Create a high-priority pending job for one repository (UI "Sync now").
    Enqueue {
        protocol_version: u32,
        token: String,
        request_id: Uuid,
        repository_id: Uuid,
    },
    CancelJob {
        protocol_version: u32,
        token: String,
        request_id: Uuid,
        job_id: Uuid,
    },
    Pause {
        protocol_version: u32,
        token: String,
        request_id: Uuid,
    },
    Resume {
        protocol_version: u32,
        token: String,
        request_id: Uuid,
    },
    Shutdown {
        protocol_version: u32,
        token: String,
        request_id: Uuid,
    },
}

/// Agent → desktop responses and push events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Pong {
        request_id: Uuid,
        protocol_version: u32,
        agent_generation: u64,
    },
    Status {
        request_id: Uuid,
        status: AgentStatus,
    },
    Ack {
        request_id: Uuid,
        ok: bool,
        message: Option<String>,
        /// Present when Enqueue created a job.
        job_id: Option<Uuid>,
    },
    Error {
        request_id: Option<Uuid>,
        code: String,
        message: String,
    },
    /// Push: forwarded from the sync-queue progress channel.
    Progress { event: serde_json::Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub generation: u64,
    pub started_at: DateTime<Utc>,
    pub protocol_version: u32,
    pub paused: bool,
    pub pending_jobs: i64,
    pub running_jobs: i64,
    pub workers: usize,
    pub last_materialize_at: Option<DateTime<Utc>>,
    pub last_materialize_created: Option<usize>,
}

impl ClientMessage {
    pub fn protocol_version(&self) -> u32 {
        match self {
            Self::Ping {
                protocol_version, ..
            }
            | Self::AgentStatus {
                protocol_version, ..
            }
            | Self::Enqueue {
                protocol_version, ..
            }
            | Self::CancelJob {
                protocol_version, ..
            }
            | Self::Pause {
                protocol_version, ..
            }
            | Self::Resume {
                protocol_version, ..
            }
            | Self::Shutdown {
                protocol_version, ..
            } => *protocol_version,
        }
    }

    pub fn token(&self) -> &str {
        match self {
            Self::Ping { token, .. }
            | Self::AgentStatus { token, .. }
            | Self::Enqueue { token, .. }
            | Self::CancelJob { token, .. }
            | Self::Pause { token, .. }
            | Self::Resume { token, .. }
            | Self::Shutdown { token, .. } => token,
        }
    }

    pub fn request_id(&self) -> Uuid {
        match self {
            Self::Ping { request_id, .. }
            | Self::AgentStatus { request_id, .. }
            | Self::Enqueue { request_id, .. }
            | Self::CancelJob { request_id, .. }
            | Self::Pause { request_id, .. }
            | Self::Resume { request_id, .. }
            | Self::Shutdown { request_id, .. } => *request_id,
        }
    }
}

pub fn parse_client_message(bytes: &[u8]) -> Result<ClientMessage, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("invalid client message: {e}"))
}

pub fn parse_server_message(bytes: &[u8]) -> Result<ServerMessage, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("invalid server message: {e}"))
}
