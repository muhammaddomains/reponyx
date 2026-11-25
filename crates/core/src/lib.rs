//! RepoNyx core: domain types and structured errors.
//!
//! This crate has no I/O, no Git, no network, and no database access.
//! It defines the vocabulary the rest of the workspace shares.

pub mod domain;
pub mod errors;
pub mod secret;

pub use domain::*;
pub use errors::{ErrorCode, RepoNyxError};
pub use secret::SecretString;
