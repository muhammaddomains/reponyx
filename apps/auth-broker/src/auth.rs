//! Random values and hashing for the Auth Broker.
//!
//! - Pairing codes: 9 characters from an unambiguous alphabet
//!   (no `0`/`O`/`1`/`I`), shown grouped as `XXX-XXX-XXX`. About
//!   45 bits of entropy; combined with the 5-attempt lockout, guessing
//!   is infeasible in the code's 10-minute lifetime.
//! - `state` tokens and device tokens: 32 random bytes, base64url.
//! - Only SHA-256 hashes of codes/tokens are stored server-side, so
//!   a dump of broker state never yields a usable code or token.

use rand::RngExt;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Constant-time equality for two SHA-256 digests. The digests are
/// fixed-length, so the comparison itself leaks nothing about where
/// (or whether) two secrets differ — a plain `==` on the digests or
/// on the hex strings short-circuits at the first differing byte
/// (audit A2).
pub fn digests_equal(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.ct_eq(b).into()
}

/// SHA-256 of a secret value, as raw bytes. Only hashes are stored
/// or compared; the raw value is never persisted server-side.
pub fn sha256_bytes(input: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hasher.finalize().into()
}

/// Alphabet without characters that are easy to confuse when a user
/// reads a code off a screen.
const PAIRING_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Generate a pairing code (`XXX-XXX-XXX` display form; the stored
/// and compared form is the 9 characters without dashes).
pub fn pairing_code() -> String {
    let mut rng = rand::rng();
    let chars: String = (0..9)
        .map(|_| {
            let index = rng.random_range(0..PAIRING_ALPHABET.len());
            PAIRING_ALPHABET[index] as char
        })
        .collect();
    format!("{}-{}-{}", &chars[0..3], &chars[3..6], &chars[6..9])
}

/// Normalize a user-entered pairing code: strip dashes/spaces and
/// uppercase, so `abc-def-ghi` and `ABCDEFGHI` are the same code.
pub fn normalize_pairing_code(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// A random 32-byte token, base64url without padding.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    base64_url(&bytes)
}

fn base64_url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

/// SHA-256 hex of a secret value. Only hashes are stored or
/// compared; the raw value is never persisted server-side.
pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_codes_are_unique_and_normalize() {
        let a = pairing_code();
        let b = pairing_code();
        assert_ne!(a, b);
        assert_eq!(a.len(), 11); // 9 chars + 2 dashes
        assert_eq!(
            normalize_pairing_code(&a.to_lowercase()),
            normalize_pairing_code(&a)
        );
        assert_eq!(normalize_pairing_code("abc-def-ghi"), "ABCDEFGHI");
    }

    #[test]
    fn hashes_are_stable_and_do_not_contain_the_value() {
        let hash = sha256_hex("test-secret-value");
        assert_eq!(hash, sha256_hex("test-secret-value"));
        assert_ne!(hash, sha256_hex("test-secret-other"));
        assert!(!hash.contains("test-secret-value"));
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn digests_compare_in_constant_time_shape() {
        // Behavioural check only (timing is not observable in a unit
        // test): equal digests match, any differing byte does not.
        let a = sha256_bytes("test-secret-value");
        assert!(digests_equal(&a, &sha256_bytes("test-secret-value")));
        assert!(!digests_equal(&a, &sha256_bytes("test-secret-other")));
        let mut b = a;
        b[31] ^= 1;
        assert!(!digests_equal(&a, &b));
    }

    #[test]
    fn random_tokens_are_unique() {
        assert_ne!(random_token(), random_token());
    }
}
