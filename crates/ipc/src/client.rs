//! Desktop-side IPC client: one request → one response over the local socket/pipe.

use crate::auth::load_shared_secret;
use crate::frame::{read_frame, write_frame};
use crate::protocol::{
    AgentStatus, ClientMessage, PROTOCOL_VERSION, ServerMessage, parse_server_message,
};
use crate::transport::{self, TransportError};
use reponyx_core::{ErrorCode, RepoNyxError};
use reponyx_credential_store::CredentialStore;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// High-level client used by the desktop Tauri backend.
pub struct IpcClient {
    data_dir: std::path::PathBuf,
    token: String,
}

impl IpcClient {
    /// Load the shared secret from the OS store (must already exist —
    /// created the first time the agent starts).
    pub fn open(
        data_dir: impl Into<std::path::PathBuf>,
        store: &Arc<dyn CredentialStore>,
    ) -> Result<Self, RepoNyxError> {
        let secret = load_shared_secret(store).map_err(RepoNyxError::from)?;
        Ok(Self {
            data_dir: data_dir.into(),
            token: secret.expose().to_string(),
        })
    }

    pub async fn ping(&self) -> Result<(u64 /* generation */, u32 /* protocol */), RepoNyxError> {
        match self
            .roundtrip(ClientMessage::Ping {
                protocol_version: PROTOCOL_VERSION,
                token: self.token.clone(),
                request_id: Uuid::new_v4(),
            })
            .await?
        {
            ServerMessage::Pong {
                agent_generation,
                protocol_version,
                ..
            } => Ok((agent_generation, protocol_version)),
            ServerMessage::Error { code, message, .. } => Err(map_ipc_error(&code, &message)),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn status(&self) -> Result<AgentStatus, RepoNyxError> {
        match self
            .roundtrip(ClientMessage::AgentStatus {
                protocol_version: PROTOCOL_VERSION,
                token: self.token.clone(),
                request_id: Uuid::new_v4(),
            })
            .await?
        {
            ServerMessage::Status { status, .. } => Ok(status),
            ServerMessage::Error { code, message, .. } => Err(map_ipc_error(&code, &message)),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn enqueue(&self, repository_id: Uuid) -> Result<Option<Uuid>, RepoNyxError> {
        match self
            .roundtrip(ClientMessage::Enqueue {
                protocol_version: PROTOCOL_VERSION,
                token: self.token.clone(),
                request_id: Uuid::new_v4(),
                repository_id,
            })
            .await?
        {
            ServerMessage::Ack {
                ok,
                message,
                job_id,
                ..
            } => {
                if ok {
                    Ok(job_id)
                } else {
                    Err(RepoNyxError::new(
                        ErrorCode::InvalidInput,
                        message.unwrap_or_else(|| "enqueue failed".into()),
                    ))
                }
            }
            ServerMessage::Error { code, message, .. } => Err(map_ipc_error(&code, &message)),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn cancel_job(&self, job_id: Uuid) -> Result<(), RepoNyxError> {
        match self
            .roundtrip(ClientMessage::CancelJob {
                protocol_version: PROTOCOL_VERSION,
                token: self.token.clone(),
                request_id: Uuid::new_v4(),
                job_id,
            })
            .await?
        {
            ServerMessage::Ack { ok, message, .. } => {
                if ok {
                    Ok(())
                } else {
                    Err(RepoNyxError::new(
                        ErrorCode::InvalidInput,
                        message.unwrap_or_else(|| "cancel failed".into()),
                    ))
                }
            }
            ServerMessage::Error { code, message, .. } => Err(map_ipc_error(&code, &message)),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn pause(&self) -> Result<(), RepoNyxError> {
        self.simple_ack(ClientMessage::Pause {
            protocol_version: PROTOCOL_VERSION,
            token: self.token.clone(),
            request_id: Uuid::new_v4(),
        })
        .await
    }

    pub async fn resume(&self) -> Result<(), RepoNyxError> {
        self.simple_ack(ClientMessage::Resume {
            protocol_version: PROTOCOL_VERSION,
            token: self.token.clone(),
            request_id: Uuid::new_v4(),
        })
        .await
    }

    pub async fn shutdown_agent(&self) -> Result<(), RepoNyxError> {
        self.simple_ack(ClientMessage::Shutdown {
            protocol_version: PROTOCOL_VERSION,
            token: self.token.clone(),
            request_id: Uuid::new_v4(),
        })
        .await
    }

    async fn simple_ack(&self, msg: ClientMessage) -> Result<(), RepoNyxError> {
        match self.roundtrip(msg).await? {
            ServerMessage::Ack { ok, message, .. } => {
                if ok {
                    Ok(())
                } else {
                    Err(RepoNyxError::new(
                        ErrorCode::InternalError,
                        message.unwrap_or_else(|| "agent rejected command".into()),
                    ))
                }
            }
            ServerMessage::Error { code, message, .. } => Err(map_ipc_error(&code, &message)),
            other => Err(unexpected(&other)),
        }
    }

    /// Open a long-lived subscription to the agent's progress stream.
    ///
    /// Connects once, authenticates with a Ping, then forwards every
    /// `ServerMessage::Progress` event into the returned broadcast
    /// channel. The connection (and its reader task) ends when the
    /// agent goes away or when the last receiver is dropped — the
    /// caller re-subscribes on the next attempt. Normal request
    /// traffic is unaffected: each `roundtrip` uses its own
    /// connection, so a push can never land in place of a reply.
    pub async fn subscribe_progress(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<serde_json::Value>, RepoNyxError> {
        let mut conn = transport::connect(&self.data_dir)
            .await
            .map_err(map_transport)?;
        let ping = serde_json::to_vec(&ClientMessage::Ping {
            protocol_version: PROTOCOL_VERSION,
            token: self.token.clone(),
            request_id: Uuid::new_v4(),
        })
        .map_err(|e| RepoNyxError::new(ErrorCode::InternalError, format!("serialize ipc: {e}")))?;
        write_frame(&mut conn, &ping).await.map_err(map_frame)?;
        let (tx, rx) = tokio::sync::broadcast::channel(256);
        tokio::spawn(async move {
            loop {
                match read_frame(&mut conn).await {
                    Ok(bytes) => match parse_server_message(&bytes) {
                        Ok(ServerMessage::Progress { event }) => {
                            if tx.send(event).is_err() {
                                // No receivers left; stop reading.
                                return;
                            }
                        }
                        // Replies (e.g. the Ping's Pong) are not
                        // progress; ignore them and keep reading.
                        Ok(_) => {}
                        Err(_) => return,
                    },
                    Err(_) => return,
                }
            }
        });
        Ok(rx)
    }

    async fn roundtrip(&self, msg: ClientMessage) -> Result<ServerMessage, RepoNyxError> {
        let mut conn = transport::connect(&self.data_dir)
            .await
            .map_err(map_transport)?;
        let body = serde_json::to_vec(&msg).map_err(|e| {
            RepoNyxError::new(ErrorCode::InternalError, format!("serialize ipc: {e}"))
        })?;
        write_frame(&mut conn, &body).await.map_err(map_frame)?;
        let resp = read_frame(&mut conn).await.map_err(map_frame)?;
        parse_server_message(&resp).map_err(|e| RepoNyxError::new(ErrorCode::InternalError, e))
    }
}

/// Probe whether the agent is accepting connections (no auth required
/// for the TCP-level connect; auth still applies on the first frame).
pub async fn agent_reachable(data_dir: &Path) -> bool {
    transport::connect(data_dir).await.is_ok()
}

fn map_transport(e: TransportError) -> RepoNyxError {
    RepoNyxError::new(
        ErrorCode::NetworkUnavailable,
        format!("background agent unavailable: {e}"),
    )
}

fn map_frame(e: crate::frame::FrameError) -> RepoNyxError {
    RepoNyxError::new(ErrorCode::NetworkUnavailable, format!("ipc frame: {e}"))
}

fn map_ipc_error(code: &str, message: &str) -> RepoNyxError {
    let mapped = match code {
        "Unauthenticated" => ErrorCode::AuthenticationExpired,
        "ProtocolMismatch" => ErrorCode::InvalidInput,
        "AgentUnavailable" => ErrorCode::NetworkUnavailable,
        _ => ErrorCode::InternalError,
    };
    RepoNyxError::new(mapped, message.to_string())
}

fn unexpected(msg: &ServerMessage) -> RepoNyxError {
    RepoNyxError::new(
        ErrorCode::InternalError,
        format!("unexpected ipc response: {msg:?}"),
    )
}
