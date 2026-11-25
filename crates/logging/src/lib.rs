//! Structured logging for RepoNyx.
//!
//! - `tracing` with an env-filter (`RepoNyx_LOG`, default `info`).
//! - Rolling daily log files under `<data_dir>/logs`.
//! - [`redact_secrets`] strips token / authorization material from any
//!   string before it is logged. Call it on process output, URLs, and
//!   error text that may contain credentials. It is a safety net, not a
//!   substitute for never putting secrets in those strings.

use std::path::Path;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Initialize logging. `log_dir` is created if missing. Safe to call
/// once per process; a second call is a no-op.
pub fn init_logging(log_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir, "reponyx.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    // Leak the guard intentionally: the process lives for its whole
    // lifetime and the writer must not be dropped at return.
    std::mem::forget(_guard);

    let filter = EnvFilter::try_from_env("RepoNyx_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stdout))
        .with(fmt::layer().with_writer(file_writer).with_ansi(false))
        .try_init()
        .map_err(|e| format!("logging already initialized or failed: {e}").into())
}

/// Register a secret value for redaction (audit A7). Pattern-based
/// redaction cannot catch a prefix-less secret (a hex setup token,
/// a base64url device token) that appears bare in a log line, so
/// processes that hold long-lived secrets register the exact values
/// and [`redact_secrets`] replaces every occurrence with
/// `[REDACTED]`.
///
/// Values shorter than 8 characters are ignored (too generic to
/// redact safely). The registry itself never prints its contents:
/// it is a private static, and no `Debug` impl exposes it. Register
/// at startup, after configuration is loaded.
pub fn register_secret(value: &str) {
    if value.len() < 8 {
        return;
    }
    let mut registered = registered_secrets();
    if !registered.iter().any(|secret| secret == value) {
        registered.push(value.to_string());
    }
}

static REGISTERED_SECRETS: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> =
    std::sync::OnceLock::new();

fn registered_secrets() -> std::sync::MutexGuard<'static, Vec<String>> {
    REGISTERED_SECRETS
        .get_or_init(|| std::sync::Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Redact credential material from a string before logging.
///
/// Handles: `Authorization: Bearer <x>`, `Authorization: token <x>`,
/// `token=<x>`, `access_token=<x>`, `password=<x>`, `client_secret=<x>`,
/// GitHub PAT-shaped tokens (`ghp_…`, `github_pat_…`), and any
/// secret registered with [`register_secret`] — including
/// prefix-less values that no pattern could match (audit A7).
/// Anything not matched is returned unchanged.
pub fn redact_secrets(input: &str) -> String {
    let mut out = input.to_string();
    // Registered values first: an exact value beats every pattern,
    // and a registered value must never survive into a pattern pass
    // that could split it.
    {
        let registered = registered_secrets();
        for secret in registered.iter() {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), "[REDACTED]");
            }
        }
    }
    for prefix in [
        "Authorization: Bearer ",
        "Authorization: token ",
        "Authorization: Basic ",
    ] {
        // Advance the search cursor past each replacement: the redacted
        // text still contains the prefix, so re-searching from the start
        // would loop forever.
        let mut search_from = 0;
        while let Some(idx) =
            find_case_insensitive(&out[search_from..], prefix).map(|i| i + search_from)
        {
            let start = idx + prefix.len();
            let end = out[start..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .map_or(out.len(), |i| start + i);
            out.replace_range(start..end, "[REDACTED]");
            search_from = start + "[REDACTED]".len();
        }
    }
    out = redact_key_value(&out, "token");
    out = redact_key_value(&out, "access_token");
    out = redact_key_value(&out, "password");
    out = redact_key_value(&out, "client_secret");
    out = redact_pat_shapes(&out);
    out
}

fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_lowercase().find(&needle.to_lowercase())
}

fn redact_key_value(input: &str, key: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    loop {
        let Some(idx) = find_case_insensitive(rest, key) else {
            out.push_str(rest);
            break;
        };
        // Only redact `key=` / `key:` assignments, not bare mentions.
        let after_key = &rest[idx + key.len()..];
        let Some(eq) = after_key.chars().next().filter(|c| *c == '=' || *c == ':') else {
            out.push_str(&rest[..idx + key.len()]);
            rest = after_key;
            continue;
        };
        let value_start = idx + key.len() + eq.len_utf8();
        let value_end = rest[value_start..]
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '&' || c == ',')
            .map_or(rest.len(), |i| value_start + i);
        out.push_str(&rest[..value_start]);
        out.push_str("[REDACTED]");
        rest = &rest[value_end..];
    }
    out
}

fn redact_pat_shapes(input: &str) -> String {
    let mut out = input.to_string();
    for prefix in ["github_pat_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        let mut result = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(idx) = rest.find(prefix) {
            let end = rest[idx..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(rest.len(), |i| idx + i);
            result.push_str(&rest[..idx]);
            result.push_str("[REDACTED]");
            rest = &rest[end..];
        }
        result.push_str(rest);
        out = result;
    }
    out
}

/// Second line of defence for the diagnostics export (Phase 9).
///
/// The export is built from already-redacted strings, but redaction is
/// pattern-based and can miss a new shape. This scan runs over the
/// finished export text and fails it outright when anything
/// secret-shaped survived, so a leaky file is never written. The
/// error names only the *kind* of pattern found, never the value.
///
/// Accepts the markers redaction produces (`[REDACTED]`) and the
/// documented placeholders (`<...>` values, `C:/path/...` examples).
/// Anything else that looks like a credential fails.
pub fn verify_no_secrets(text: &str) -> Result<(), &'static str> {
    // PEM private-key material.
    if text.contains("-----BEGIN ") && text.contains("PRIVATE KEY-----") {
        return Err("private key block");
    }
    // GitHub token shapes with a real-looking value attached.
    for prefix in ["github_pat_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        let mut rest = text;
        while let Some(idx) = rest.find(prefix) {
            let after = &rest[idx + prefix.len()..];
            let value_len = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .count();
            if value_len >= 10 {
                return Err("github token shape");
            }
            rest = after;
        }
    }
    // `Authorization: Bearer/Token/Basic <value>` with a live value.
    verify_auth_header(text)?;
    // `key=<value>` assignments for credential keys.
    for key in [
        "token",
        "access_token",
        "password",
        "client_secret",
        "TAURI_SIGNING_PRIVATE_KEY",
    ] {
        verify_key_value(text, key)?;
    }
    // Basic-auth credentials embedded in a URL.
    verify_url_credentials(text)?;
    Ok(())
}

/// A value is safe when redaction already replaced it, or it is a
/// documented placeholder rather than a real credential.
fn value_is_safe(value: &str) -> bool {
    let lower = value.to_lowercase();
    value.is_empty()
        || lower.starts_with("[redacted]")
        || (value.starts_with('<') && value.ends_with('>'))
        || lower.starts_with("c:/path/")
        || lower.starts_with("c:\\path\\")
}

fn verify_auth_header(text: &str) -> Result<(), &'static str> {
    // Work on the lowercased copy throughout: byte indices from a
    // case-insensitive search are only valid in the string that was
    // searched.
    let lower = text.to_lowercase();
    let mut rest = lower.as_str();
    while let Some(idx) = rest.find("authorization:") {
        let after = rest[idx + "authorization:".len()..].trim_start();
        for scheme in ["bearer ", "token ", "basic "] {
            if let Some(value) = after.strip_prefix(scheme) {
                let value = value
                    .split(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                    .next()
                    .unwrap_or("");
                if !value_is_safe(value) {
                    return Err("authorization header");
                }
            }
        }
        rest = &rest[idx + "authorization:".len()..];
    }
    Ok(())
}

fn verify_key_value(text: &str, key: &str) -> Result<(), &'static str> {
    let lower = text.to_lowercase();
    let key = key.to_lowercase();
    let mut rest = lower.as_str();
    loop {
        let Some(idx) = rest.find(key.as_str()) else {
            return Ok(());
        };
        let after_key = &rest[idx + key.len()..];
        let Some(eq) = after_key.chars().next().filter(|c| *c == '=' || *c == ':') else {
            rest = after_key;
            continue;
        };
        let value_start = idx + key.len() + eq.len_utf8();
        let value_end = rest[value_start..]
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '&' || c == ',')
            .map_or(rest.len(), |i| value_start + i);
        if !value_is_safe(&rest[value_start..value_end]) {
            return Err("credential key=value");
        }
        rest = &rest[value_end..];
    }
}

fn verify_url_credentials(text: &str) -> Result<(), &'static str> {
    let lower = text.to_lowercase();
    for marker in ["://x-access-token:", "://x-oauth-basic:"] {
        let mut rest = lower.as_str();
        while let Some(idx) = rest.find(marker) {
            let after = &rest[idx + marker.len()..];
            let value = after
                .split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '@' || c == '/')
                .next()
                .unwrap_or("");
            if !value_is_safe(value) {
                return Err("credentials in url");
            }
            rest = after;
        }
    }
    Ok(())
}

/// The last `count` lines of a text file, for the diagnostics log
/// tail. A missing file is an error; a short file returns what it
/// has. Invalid UTF-8 is read lossily rather than failing the
/// whole export.
pub fn tail_lines(path: &Path, count: usize) -> std::io::Result<Vec<String>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let start = lines.len().saturating_sub(count);
    Ok(lines[start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_bearer_header() {
        let s = redact_secrets("Authorization: Bearer abc123xyz called");
        assert!(!s.contains("abc123xyz"));
        assert!(s.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_token_key_value() {
        let s = redact_secrets("failed with token=supersecret123 end");
        assert!(!s.contains("supersecret123"));
    }

    #[test]
    fn redacts_github_pat_shapes() {
        let s = redact_secrets("used ghp_AbC123xYz and github_pat_11ABCxyz_0def");
        assert!(!s.contains("ghp_AbC123xYz"));
        assert!(!s.contains("github_pat_11ABCxyz_0def"));
    }

    #[test]
    fn redacts_registered_prefix_less_secret() {
        // A hex setup token / base64url device token has no prefix
        // for the pattern redactors to catch (audit A7); registering
        // the exact value redacts it even when it appears bare.
        register_secret("9f2c41ab77e0d3c5a1b8");
        let s = redact_secrets("saw 9f2c41ab77e0d3c5a1b8 in the output");
        assert!(!s.contains("9f2c41ab77e0d3c5a1b8"));
        assert!(s.contains("[REDACTED]"));
    }

    #[test]
    fn leaves_benign_text_unchanged() {
        let s = redact_secrets("fetch completed for owner/repo in 123ms");
        assert_eq!(s, "fetch completed for owner/repo in 123ms");
    }

    #[test]
    fn verify_accepts_redacted_text() {
        let redacted = redact_secrets(
            "Authorization: Bearer abc123xyz and token=supersecret123, used ghp_AbC123xYz4567890",
        );
        assert!(verify_no_secrets(&redacted).is_ok());
    }

    #[test]
    fn verify_accepts_benign_text() {
        assert!(verify_no_secrets("fetch completed for owner/repo in 123ms").is_ok());
        assert!(verify_no_secrets("mirrors: 3 repositories, 42 MB").is_ok());
    }

    #[test]
    fn verify_accepts_placeholders() {
        assert!(verify_no_secrets("TAURI_SIGNING_PRIVATE_KEY=C:/path/to/your/reponyx.key").is_ok());
        assert!(verify_no_secrets("TAURI_SIGNING_PRIVATE_KEY_PASSWORD=<redacted>").is_ok());
    }

    #[test]
    fn verify_rejects_raw_pat() {
        let err = verify_no_secrets("saw ghp_AbC123xYz4567890 in output").unwrap_err();
        assert_eq!(err, "github token shape");
    }

    #[test]
    fn verify_rejects_raw_bearer() {
        let err = verify_no_secrets("Authorization: Bearer secretvalue123").unwrap_err();
        assert_eq!(err, "authorization header");
    }

    #[test]
    fn verify_rejects_private_key_block() {
        let err = verify_no_secrets("key:\n-----BEGIN RSA PRIVATE KEY-----\nMIIE...").unwrap_err();
        assert_eq!(err, "private key block");
    }

    #[test]
    fn verify_rejects_signing_key_value() {
        let err = verify_no_secrets("TAURI_SIGNING_PRIVATE_KEY=realkeycontent123").unwrap_err();
        assert_eq!(err, "credential key=value");
    }

    #[test]
    fn verify_rejects_url_credentials() {
        // Rejected either as a `token:` assignment or as URL
        // credentials — both are secret-shaped; the kind only says
        // which check fired first.
        assert!(
            verify_no_secrets("remote https://x-access-token:secretvalue123@github.com/o/r.git")
                .is_err()
        );
        assert!(
            verify_no_secrets("https://x-oauth-basic:secretvalue123@github.com/o/r.git").is_err()
        );
    }

    #[test]
    fn tail_lines_returns_last_n() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let content = (1..=10)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(file.path(), content).unwrap();
        let tail = tail_lines(file.path(), 3).unwrap();
        assert_eq!(tail, vec!["line 8", "line 9", "line 10"]);
        let all = tail_lines(file.path(), 50).unwrap();
        assert_eq!(all.len(), 10);
        // `NamedTempFile` deletes the file on drop.
        file.close().unwrap();
    }

    #[test]
    fn tail_lines_missing_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.txt");
        assert!(tail_lines(&path, 5).is_err());
    }
}
