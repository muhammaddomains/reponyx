//! Re-export of [`SecretString`] from `reponyx-core`, kept so
//! existing `crate::SecretString` imports keep working.
//!
//! The type lives in core because the Auth Broker needs it too
//! (setup token, private key) without depending on the OS
//! credential-store crate.

pub use reponyx_core::SecretString;
