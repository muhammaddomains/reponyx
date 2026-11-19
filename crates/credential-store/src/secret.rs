//! A secret value held in memory: zeroed on drop, never printed.
//!
//! Use [`SecretString::expose`] only at the exact point the value is
//! needed (an `Authorization` header, a credential-helper env var),
//! then drop it. `Debug` prints `<redacted>`; `Display` is
//! deliberately not implemented, so a secret can never be formatted
//! into a log line by accident.

use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretString {
    value: String,
}

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// The raw value. Callers must treat the return as secret: use it
    /// only to build an `Authorization` header or hand it to a
    /// credential helper, then drop it. Never log it, never put it in
    /// an error message, never store it in SQLite or a file.
    pub fn expose(&self) -> &str {
        &self.value
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString(<redacted>)")
    }
}

impl PartialEq for SecretString {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for SecretString {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_value() {
        let secret = SecretString::new("test-pat-super-secret-value");
        let debug = format!("{secret:?}");
        assert!(!debug.contains("test-pat-super-secret-value"));
        assert!(debug.contains("<redacted>"));
    }
}
