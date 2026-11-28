//! Auth Broker library: configuration, state, pairing, token
//! minting, audit, rate limiting, and HTTP routes.
//!
//! The broker holds NO GitHub App private key (W4): the web is the
//! single holder of the key and mints installation tokens, and the
//! broker asks the web for them with the user's web session. The
//! broker is a separate binary (`apps/auth-broker`), not a
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
