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
}

impl Listener {
    pub async fn accept(&self) -> Result<Conn, TransportError> {
        #[cfg(unix)]
        {
            let (stream, _) = self.inner.accept().await?;
            Ok(Box::new(stream))
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            // Windows named pipes: create a new server instance per accept.
            let server = ServerOptions::new()
                .first_pipe_instance(false)
                .create(&self.path)
                .map_err(TransportError::Io)?;
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
        // Create the first instance so the pipe exists; subsequent
        // accepts create additional instances.
        let _server = ServerOptions::new()
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
        Ok(Listener { path })
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
