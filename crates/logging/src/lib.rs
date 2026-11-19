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

/// Redact credential material from a string before logging.
///
/// Handles: `Authorization: Bearer <x>`, `Authorization: token <x>`,
/// `token=<x>`, `access_token=<x>`, `password=<x>`, `client_secret=<x>`,
/// and GitHub PAT-shaped tokens (`ghp_…`, `github_pat_…`).
/// Anything not matched is returned unchanged.
pub fn redact_secrets(input: &str) -> String {
    let mut out = input.to_string();
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
    fn leaves_benign_text_unchanged() {
        let s = redact_secrets("fetch completed for owner/repo in 123ms");
        assert_eq!(s, "fetch completed for owner/repo in 123ms");
    }
}
