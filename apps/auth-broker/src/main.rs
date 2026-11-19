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
//! - `REPONYX_BROKER_GITHUB_API_URL` — GitHub API base (default
//!   `https://api.github.com`)
//!
//! The broker logs method/path/status only — never tokens, codes,
//! keys, or request bodies.

use reponyx_auth_broker::config::BrokerConfig;
use reponyx_auth_broker::{BrokerState, app};
use std::net::SocketAddr;
use tracing::info;

#[tokio::main]
async fn main() {
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
    let bind_addr: SocketAddr = config.bind_addr;
    let state = BrokerState::new(config);
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
