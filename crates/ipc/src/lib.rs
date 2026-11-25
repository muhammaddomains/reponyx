//! Local IPC between the desktop UI and the background agent (Phase 7).
//!
//! Transport:
//! - Windows: named pipe `\\.\pipe\reponyx-agent`
//! - Unix: domain socket under the data dir (`ipc/agent.sock`)
//!
//! Protocol: length-prefixed JSON frames (`u32` little-endian length +
//! UTF-8 JSON body). Every request carries `protocol_version` and a
//! HMAC-style shared secret (or bearer token) established at first
//! agent start and stored in the OS credential store.
//!
//! This crate never opens a TCP port and never logs message payloads
//! that may contain repository identifiers beyond what tracing needs
//! (message type only by default).

pub mod agent_lock;
pub mod auth;
pub mod client;
pub mod frame;
#[cfg(windows)]
pub mod pipe_security;
pub mod protocol;
pub mod server;
pub mod transport;

pub use agent_lock::{
    AgentLock, lock_path, pid_alive, pid_is_agent, read_lock, remove_lock, write_lock,
};

pub use auth::{
    IpcAuthError, SERVICE_IPC, ensure_shared_secret, load_shared_secret, store_shared_secret,
};
pub use client::{IpcClient, agent_reachable};
pub use frame::{FrameError, read_frame, write_frame};
pub use protocol::{
    AgentStatus, ClientMessage, PROTOCOL_VERSION, ServerMessage, parse_client_message,
    parse_server_message,
};
pub use server::{AgentCommand, AgentIpcServer, IpcServerConfig};
pub use transport::{TransportError, connect, listen, socket_path};
