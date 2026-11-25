//! Auth Broker entry point: load config from the environment, then
//! serve the HTTP API on the configured (loopback) address.
//!
//! Configuration (all via environment; see `README.md`):
//! - `REPONYX_BROKER_APP_ID` — GitHub App id (required)
//! - `REPONYX_BROKER_APP_SLUG` — GitHub App slug (default `reponyx`)
//! - `REPONYX_BROKER_PRIVATE_KEY` or `REPONYX_BROKER_PRIVATE_KEY_PATH`
//!   — the App's private key (required)
//! - `REPONYX_BROKER_SETUP_TOKEN` — operator token for pairing
//!   requests (required, at least 16 characters)
//! - `REPONYX_BROKER_BIND` — bind address (default `127.0.0.1:8765`)
//! - `REPONYX_BROKER_ALLOW_NON_LOOPBACK` — set to `true` to allow a
//!   non-loopback bind (audit B2). Without it the broker refuses to
//!   bind anywhere but loopback: a LAN-visible broker would expose
//!   setup-token auth to the whole network.
//! - `REPONYX_BROKER_GITHUB_API_URL` — GitHub API base (default
//!   `https://api.github.com`)
//! - `REPONYX_BROKER_ENV_FILE` — explicit `.env` file to load
//!   first (the desktop sets this when it spawns the broker for an
//!   installed app, pointing at `<data_dir>/.env`); a missing file
//!   is ignored, and a CWD `.env` still fills in the rest.
//! - `REPONYX_BROKER_DATA_DIR` — durable state directory (default
//!   `broker-data`); holds `broker.sqlite`, the installations,
//!   pairing codes, setup states, device tokens, and audit log.
//!   Only hashes of codes/tokens are stored — never the raw values,
//!   and never the private key.
//!
//! The broker also reads a `.env` file from the working directory at
//! startup (see `.env.example`), so `pnpm broker` / `pnpm dev:all`
//! work without exporting variables first. Real OS environment
//! variables always win: dotenv never overwrites a variable that is
//! already set. `.env` is gitignored — never commit it.
//!
//! Starting a second broker while one is already listening on the
//! bind address is not an error: the new process prints "already
//! running" and exits 0, so `dev:all` (and the desktop's spawn)
//! never kills the whole stack over a duplicate broker.
//!
//! The broker logs method/path/status only — never tokens, codes,
//! keys, or request bodies.

use reponyx_auth_broker::BrokerStore;
use reponyx_auth_broker::config::BrokerConfig;
use reponyx_auth_broker::{BrokerState, app};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::info;

#[tokio::main]
async fn main() {
    // Load `.env` from the explicit file first (when the desktop
    // spawns the broker it points here), then from the working
    // directory, so the broker can run from `pnpm broker` /
    // `pnpm dev:all` without exported variables. Real OS environment
    // variables are never overwritten by the file (dotenv only fills
    // in missing ones), and a missing file is not an error —
    // `BrokerConfig::from_env` still enforces the required
    // variables below.
    if let Ok(path) = std::env::var("REPONYX_BROKER_ENV_FILE")
        && !path.trim().is_empty()
    {
        let _ = dotenvy::from_path(&path);
    }
    dotenvy::dotenv().ok();
    let log_dir = std::env::var("REPONYX_BROKER_LOG_DIR").unwrap_or_else(|_| "logs".to_string());
    if let Err(err) = reponyx_logging::init_logging(std::path::Path::new(&log_dir)) {
        eprintln!(
            "auth-broker: logging to {log_dir} unavailable ({err}); continuing with no log output"
        );
    }
    let config = match BrokerConfig::from_env() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("auth-broker: cannot start: {message}");
            std::process::exit(2);
        }
    };
    // Register the long-lived secrets for redaction (audit A7): a
    // prefix-less setup token or private key that ever appears bare
    // in a log line is replaced with `[REDACTED]`, even though no
    // pattern could match it.
    reponyx_logging::register_secret(config.setup_token.expose());
    reponyx_logging::register_secret(config.private_key_pem.expose());
    // The broker's secrets come from a `.env` file by default: warn
    // when that file is readable by other users (audit A4). On
    // Windows the check is skipped — ACLs, not modes, govern access
    // there, and the file lives under the user's %APPDATA%.
    warn_if_env_file_world_readable();
    let bind_addr: SocketAddr = config.bind_addr;
    // A broker is already listening here (started by hand, by
    // dev:all, or by the desktop): this is not an error. Exit 0 so
    // `concurrently -k` never kills the whole stack over a duplicate
    // broker, and so the desktop's spawn is a harmless no-op.
    if broker_already_running(bind_addr).await {
        eprintln!("auth-broker: already running at {bind_addr}; exiting");
        return;
    }
    let db_path = config.data_dir.join("broker.sqlite");
    let store = match BrokerStore::open(&db_path).await {
        Ok(store) => std::sync::Arc::new(store),
        Err(err) => {
            eprintln!(
                "auth-broker: cannot open broker store at {}: {err}",
                db_path.display()
            );
            std::process::exit(2);
        }
    };
    let state = match BrokerState::open(config, store).await {
        Ok(state) => state,
        Err(err) => {
            eprintln!("auth-broker: cannot load broker state: {err}");
            std::process::exit(2);
        }
    };
    info!(bind_addr = %bind_addr, "auth-broker listening");
    let listener = match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("auth-broker: cannot bind {bind_addr}: {err}");
            std::process::exit(2);
        }
    };
    if let Err(err) = axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    {
        eprintln!("auth-broker: server error: {err}");
        std::process::exit(1);
    }
}

/// Warn when a `.env` file the broker loads is readable by users
/// other than the owner (audit A4). The broker's setup token and
/// private key live in that file; a world-readable copy defeats
/// every other secret control. Unix only: on Windows the file lives
/// under the user's profile and access is governed by ACLs.
#[cfg(unix)]
fn warn_if_env_file_world_readable() {
    use std::os::unix::fs::PermissionsExt;
    let mut paths = Vec::new();
    if let Ok(path) = std::env::var("REPONYX_BROKER_ENV_FILE")
        && !path.trim().is_empty()
    {
        paths.push(std::path::PathBuf::from(path));
    }
    paths.push(std::path::PathBuf::from(".env"));
    for path in paths {
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        let mode = metadata.permissions().mode();
        if mode & 0o044 != 0 {
            eprintln!(
                "auth-broker: WARNING: {} is readable by other users (mode {:o}); run `chmod 600 {}` so only you can read the setup token and private key",
                path.display(),
                mode & 0o777,
                path.display()
            );
        }
    }
}

#[cfg(not(unix))]
fn warn_if_env_file_world_readable() {}

/// Is a broker already answering `/health` on this address? A bare
/// TCP connect is not enough — another process could hold the port —
/// so this sends a minimal HTTP GET and requires a 200 status line.
/// Dependency-free on purpose: the broker binary does not need an
/// HTTP client just for this probe.
async fn broker_already_running(bind_addr: SocketAddr) -> bool {
    let connect = tokio::time::timeout(
        Duration::from_secs(1),
        tokio::net::TcpStream::connect(bind_addr),
    )
    .await;
    let Ok(Ok(mut stream)) = connect else {
        return false;
    };
    let request = format!("GET /health HTTP/1.1\r\nHost: {bind_addr}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).await.is_err() {
        return false;
    }
    let mut buf = [0u8; 256];
    let read = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buf)).await;
    let Ok(Ok(n)) = read else {
        return false;
    };
    std::str::from_utf8(&buf[..n])
        .map(|text| text.contains(" 200 "))
        .unwrap_or(false)
}
