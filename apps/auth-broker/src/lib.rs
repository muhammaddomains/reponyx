//! Auth Broker library: configuration, state, pairing, token
//! minting, audit, rate limiting, and HTTP routes.
//!
//! The broker is the ONLY component that ever holds the GitHub App
//! private key. It is a separate binary (`apps/auth-broker`), not a
//! dependency of the desktop crate — see
//! `scripts/check-no-private-key.sh`.

pub mod audit;
pub mod auth;
pub mod config;
pub mod rate_limit;
pub mod routes;
pub mod state;
pub mod store;
pub mod tokens;

pub use routes::app;
pub use state::BrokerState;
pub use store::BrokerStore;
