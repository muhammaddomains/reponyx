//! Platform transport: Unix domain sockets / Windows named pipes.

use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Other(String),
}

/// Resolved path of the agent endpoint (socket file or logical pipe name).
pub fn socket_path(data_dir: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let _ = data_dir;
        PathBuf::from(r"\\.\pipe\reponyx-agent")
    }
    #[cfg(not(windows))]
    {
        data_dir.join("ipc").join("agent.sock")
    }
}

/// Abstract connection handle (read + write half of the same stream).
pub type Conn = Box<dyn AsyncStream>;

pub trait AsyncStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncStream for T {}

/// Accept loop listener.
pub struct Listener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    path: PathBuf,
    /// The pipe instance currently waiting for a client. Kept (never
    /// dropped while idle) so a listening instance always exists: a
    /// bare probe must never land on a missing pipe.
    #[cfg(windows)]
    pending: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
}

impl Listener {
    pub async fn accept(&mut self) -> Result<Conn, TransportError> {
        #[cfg(unix)]
        {
            let (stream, _) = self.inner.accept().await?;
            Ok(Box::new(stream))
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            // Take the pending instance (creating one on demand when
            // none is pending), then create its replacement BEFORE
            // awaiting connect — so a listening instance is always
            // available for the next client.
            let server = match self.pending.take() {
                Some(server) => server,
                None => ServerOptions::new()
                    .first_pipe_instance(false)
                    .create(&self.path)
                    .map_err(TransportError::Io)?,
            };
            if let Ok(next) = ServerOptions::new()
                .first_pipe_instance(false)
                .create(&self.path)
            {
                self.pending = Some(next);
            }
            // If the replacement could not be created, the next
            // accept() creates one on demand; this connection still
            // works.
            server.connect().await.map_err(TransportError::Io)?;
            Ok(Box::new(server))
        }
    }
}

pub async fn listen(data_dir: &Path) -> Result<Listener, TransportError> {
    #[cfg(unix)]
    {
        let path = socket_path(data_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() {
            // Stale socket from a previous crash — remove so bind succeeds.
            let _ = std::fs::remove_file(&path);
        }
        let listener = tokio::net::UnixListener::bind(&path)?;
        // Restrict to the current user (0700 on the directory is already
        // typical for data_dir; the socket file inherits umask).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(Listener { inner: listener })
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ServerOptions;
        let path = socket_path(data_dir);
        // Create the first instance and KEEP it as the pending
        // listening instance. (The old code dropped it here, so the
        // pipe did not exist until the first accept() created one —
        // a probe landing in that gap, or between accept() returning
        // and the next accept() creating an instance, saw no pipe
        // at all and the desktop concluded the agent was down.)
        let pending = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&path)
            .map_err(|e| {
                // `first_pipe_instance` fails with Access denied when
                // an instance of the pipe already exists — i.e. another
                // agent is already running and owns the pipe. That is
                // the normal single-instance conflict, not a permissions
                // problem, so name it instead of surfacing the raw OS
                // error ("io error: Access is denied. (os error 5)").
                if e.kind() == io::ErrorKind::PermissionDenied {
                    TransportError::Other(
                        "named pipe already in use (Access is denied) — another reponyx-agent is probably already running; only one agent can own the pipe"
                            .into(),
                    )
                } else {
                    TransportError::Io(e)
                }
            })?;
        Ok(Listener {
            path,
            pending: Some(pending),
        })
    }
}

pub async fn connect(data_dir: &Path) -> Result<Conn, TransportError> {
    #[cfg(unix)]
    {
        let path = socket_path(data_dir);
        let stream = tokio::net::UnixStream::connect(&path).await?;
        Ok(Box::new(stream))
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;
        let path = socket_path(data_dir);
        let client = ClientOptions::new()
            .open(&path)
            .map_err(TransportError::Io)?;
        Ok(Box::new(client))
    }
}
