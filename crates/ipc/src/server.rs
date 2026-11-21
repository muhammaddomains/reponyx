//! Agent-side IPC server: accept connections, auth, dispatch commands.

use crate::auth::{ensure_shared_secret, token_matches};
use crate::frame::{read_frame, write_frame};
use crate::protocol::{
    AgentStatus, ClientMessage, PROTOCOL_VERSION, ServerMessage, parse_client_message,
};
use crate::transport::{self, Listener, TransportError};
use reponyx_credential_store::CredentialStore;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Commands the IPC layer asks the agent runtime to perform.
#[derive(Debug)]
pub enum AgentCommand {
    Enqueue {
        request_id: Uuid,
        repository_id: Uuid,
        reply: tokio::sync::oneshot::Sender<ServerMessage>,
    },
    CancelJob {
        request_id: Uuid,
        job_id: Uuid,
        reply: tokio::sync::oneshot::Sender<ServerMessage>,
    },
    Pause {
        request_id: Uuid,
        reply: tokio::sync::oneshot::Sender<ServerMessage>,
    },
    Resume {
        request_id: Uuid,
        reply: tokio::sync::oneshot::Sender<ServerMessage>,
    },
    Shutdown {
        request_id: Uuid,
        reply: tokio::sync::oneshot::Sender<ServerMessage>,
    },
    Status {
        request_id: Uuid,
        reply: tokio::sync::oneshot::Sender<ServerMessage>,
    },
}

#[derive(Debug, Clone)]
pub struct IpcServerConfig {
    /// Monotonic generation id; bumped on each agent process start.
    pub generation: u64,
}

impl Default for IpcServerConfig {
    fn default() -> Self {
        Self { generation: 1 }
    }
}

/// Shared flags the agent updates and status queries read.
pub struct AgentRuntimeFlags {
    pub paused: AtomicBool,
    pub generation: AtomicU64,
}

impl AgentRuntimeFlags {
    pub fn new(generation: u64) -> Self {
        Self {
            paused: AtomicBool::new(false),
            generation: AtomicU64::new(generation),
        }
    }
}

pub struct AgentIpcServer {
    listener: Listener,
    expected_token: String,
    generation: u64,
    commands: mpsc::Sender<AgentCommand>,
    /// Optional progress fan-out: JSON-encoded SyncProgressEvent values.
    progress_rx: Arc<RwLock<Option<tokio::sync::broadcast::Receiver<serde_json::Value>>>>,
}

impl AgentIpcServer {
    pub async fn bind(
        data_dir: &Path,
        store: &Arc<dyn CredentialStore>,
        generation: u64,
        commands: mpsc::Sender<AgentCommand>,
    ) -> Result<Self, TransportError> {
        let secret = ensure_shared_secret(store)
            .map_err(|e| TransportError::Other(format!("ipc secret: {e}")))?;
        let listener = transport::listen(data_dir).await?;
        info!(endpoint = %transport::socket_path(data_dir).display(), "ipc listening");
        Ok(Self {
            listener,
            expected_token: secret.expose().to_string(),
            generation,
            commands,
            progress_rx: Arc::new(RwLock::new(None)),
        })
    }

    /// Attach a progress source (agent wires the queue broadcast here).
    pub async fn set_progress_source(
        &self,
        rx: tokio::sync::broadcast::Receiver<serde_json::Value>,
    ) {
        *self.progress_rx.write().await = Some(rx);
    }

    /// Accept loop until `cancel` is cancelled.
    pub async fn run(self, cancel: tokio_util::sync::CancellationToken) {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("ipc server shutting down");
                    break;
                }
                accepted = self.listener.accept() => {
                    match accepted {
                        Ok(conn) => {
                            let token = self.expected_token.clone();
                            let generation = self.generation;
                            let commands = self.commands.clone();
                            let cancel = cancel.child_token();
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(conn, token, generation, commands, cancel).await {
                                    debug!(error = %e, "ipc connection ended");
                                }
                            });
                        }
                        Err(e) => {
                            warn!(error = %e, "ipc accept failed");
                        }
                    }
                }
            }
        }
    }
}

async fn handle_connection(
    mut conn: transport::Conn,
    expected_token: String,
    generation: u64,
    commands: mpsc::Sender<AgentCommand>,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            frame = read_frame(&mut conn) => {
                let body = match frame {
                    Ok(b) => b,
                    Err(crate::frame::FrameError::Closed) => return Ok(()),
                    Err(e) => return Err(e.to_string()),
                };
                let msg = parse_client_message(&body)?;
                let reply = dispatch(msg, &expected_token, generation, &commands).await;
                let bytes = serde_json::to_vec(&reply).map_err(|e| e.to_string())?;
                write_frame(&mut conn, &bytes).await.map_err(|e| e.to_string())?;
            }
        }
    }
}

async fn dispatch(
    msg: ClientMessage,
    expected_token: &str,
    generation: u64,
    commands: &mpsc::Sender<AgentCommand>,
) -> ServerMessage {
    let request_id = msg.request_id();
    if msg.protocol_version() != PROTOCOL_VERSION {
        return ServerMessage::Error {
            request_id: Some(request_id),
            code: "ProtocolMismatch".into(),
            message: format!(
                "unsupported protocol version {}; agent speaks {}",
                msg.protocol_version(),
                PROTOCOL_VERSION
            ),
        };
    }
    if !token_matches(msg.token(), expected_token) {
        return ServerMessage::Error {
            request_id: Some(request_id),
            code: "Unauthenticated".into(),
            message: "invalid ipc token".into(),
        };
    }

    match msg {
        ClientMessage::Ping { request_id, .. } => ServerMessage::Pong {
            request_id,
            protocol_version: PROTOCOL_VERSION,
            agent_generation: generation,
        },
        ClientMessage::AgentStatus { request_id, .. } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if commands
                .send(AgentCommand::Status {
                    request_id,
                    reply: tx,
                })
                .await
                .is_err()
            {
                return ServerMessage::Error {
                    request_id: Some(request_id),
                    code: "AgentUnavailable".into(),
                    message: "agent command channel closed".into(),
                };
            }
            rx.await.unwrap_or(ServerMessage::Error {
                request_id: Some(request_id),
                code: "AgentUnavailable".into(),
                message: "no status reply".into(),
            })
        }
        ClientMessage::Enqueue {
            request_id,
            repository_id,
            ..
        } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if commands
                .send(AgentCommand::Enqueue {
                    request_id,
                    repository_id,
                    reply: tx,
                })
                .await
                .is_err()
            {
                return unavailable(request_id);
            }
            rx.await.unwrap_or_else(|_| unavailable(request_id))
        }
        ClientMessage::CancelJob {
            request_id, job_id, ..
        } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if commands
                .send(AgentCommand::CancelJob {
                    request_id,
                    job_id,
                    reply: tx,
                })
                .await
                .is_err()
            {
                return unavailable(request_id);
            }
            rx.await.unwrap_or_else(|_| unavailable(request_id))
        }
        ClientMessage::Pause { request_id, .. } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if commands
                .send(AgentCommand::Pause {
                    request_id,
                    reply: tx,
                })
                .await
                .is_err()
            {
                return unavailable(request_id);
            }
            rx.await.unwrap_or_else(|_| unavailable(request_id))
        }
        ClientMessage::Resume { request_id, .. } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if commands
                .send(AgentCommand::Resume {
                    request_id,
                    reply: tx,
                })
                .await
                .is_err()
            {
                return unavailable(request_id);
            }
            rx.await.unwrap_or_else(|_| unavailable(request_id))
        }
        ClientMessage::Shutdown { request_id, .. } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if commands
                .send(AgentCommand::Shutdown {
                    request_id,
                    reply: tx,
                })
                .await
                .is_err()
            {
                return unavailable(request_id);
            }
            rx.await.unwrap_or_else(|_| unavailable(request_id))
        }
    }
}

fn unavailable(request_id: Uuid) -> ServerMessage {
    ServerMessage::Error {
        request_id: Some(request_id),
        code: "AgentUnavailable".into(),
        message: "agent command channel closed".into(),
    }
}

// Silence unused import when AgentStatus is only constructed by the agent binary.
#[allow(dead_code)]
fn _status_placeholder() -> Option<AgentStatus> {
    None
}
