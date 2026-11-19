//! GitHub REST pagination: `Link` header parsing and opaque cursors.
//!
//! GitHub paginates with `Link: <url>; rel="next", <url>; rel="last"`
//! headers. The cursor handed to callers is the absolute next-page URL
//! itself — opaque to callers, but trivially resumable: a discovery
//! interrupted by a rate limit resumes from exactly the same URL.
//!
//! Decisions (all tested):
//! - A missing `Link` header means a single page (no next page).
//! - A malformed `Link` header never crashes discovery: if no
//!   well-formed `rel="next"` entry can be found, the page is treated
//!   as the last one. A malformed header is a server bug, not a reason
//!   to lose the repos already fetched.
//! - A cursor that does not point at the configured API base is
//!   rejected (`InvalidInput`): the client must never follow a
//!   next-page URL to an arbitrary host (e.g. a compromised mock or a
//!   misconfigured base URL).

use reponyx_core::{ErrorCode, RepoNyxError};

/// Extract the `rel="next"` URL from a `Link` header value.
///
/// Returns `None` when the header is absent, empty, or contains no
/// well-formed next link. Parsing is deliberately lenient about
/// whitespace and about extra link parameters; it is strict about
/// the shape `<url>; rel="next"`.
pub fn parse_next_link(link_header: Option<&str>) -> Option<String> {
    let header = link_header?;
    for part in header.split(',') {
        let part = part.trim();
        let Some(url) = part
            .strip_prefix('<')
            .and_then(|rest| rest.split('>').next().map(str::to_string))
        else {
            continue;
        };
        let params = part.split('>').nth(1).unwrap_or("");
        for param in params.split(';') {
            if param.trim() == "rel=\"next\"" {
                return Some(url);
            }
        }
    }
    None
}

/// Validate that a cursor (a next-page URL) points at the configured
/// API base. Accepts absolute URLs under `base_url` and root-relative
/// paths (`/user/repos?...`), which are resolved against the base.
/// A plain prefix match is not enough: `https://api.github.com.evil…`
/// shares a prefix with the base but is a different host, so the
/// cursor must equal the base or continue it with a `/`.
pub fn resolve_cursor(base_url: &str, cursor: &str) -> Result<String, RepoNyxError> {
    let base = base_url.trim_end_matches('/');
    if cursor.starts_with('/') {
        return Ok(format!("{base}{cursor}"));
    }
    if cursor == base || cursor.starts_with(&format!("{base}/")) {
        return Ok(cursor.to_string());
    }
    Err(RepoNyxError::new(
        ErrorCode::InvalidInput,
        "pagination cursor does not point at the configured GitHub API",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_next_link() {
        let header = "<https://api.github.com/user/repos?page=2>; rel=\"next\", <https://api.github.com/user/repos?page=5>; rel=\"last\"";
        assert_eq!(
            parse_next_link(Some(header)).as_deref(),
            Some("https://api.github.com/user/repos?page=2")
        );
    }

    #[test]
    fn missing_link_header_is_last_page() {
        assert_eq!(parse_next_link(None), None);
        assert_eq!(parse_next_link(Some("")), None);
    }

    #[test]
    fn link_without_next_is_last_page() {
        let header = "<https://api.github.com/user/repos?page=1>; rel=\"prev\", <https://api.github.com/user/repos?page=1>; rel=\"first\"";
        assert_eq!(parse_next_link(Some(header)), None);
    }

    #[test]
    fn malformed_link_header_is_last_page_not_a_crash() {
        assert_eq!(parse_next_link(Some("garbage, no angles here")), None);
        assert_eq!(
            parse_next_link(Some("<https://example.com>; rel=next")),
            None
        );
        assert_eq!(parse_next_link(Some("<https://example.com>")), None);
        // A well-formed next link still wins even with junk around it.
        assert_eq!(
            parse_next_link(Some(
                "junk, <https://api.github.com/x?page=2>; rel=\"next\""
            ))
            .as_deref(),
            Some("https://api.github.com/x?page=2")
        );
    }

    #[test]
    fn cursor_must_point_at_the_api_base() {
        let base = "https://api.github.com";
        assert_eq!(
            resolve_cursor(base, "https://api.github.com/user/repos?page=2").unwrap(),
            "https://api.github.com/user/repos?page=2"
        );
        assert_eq!(
            resolve_cursor(base, "/user/repos?page=2").unwrap(),
            "https://api.github.com/user/repos?page=2"
        );
        assert!(resolve_cursor(base, "https://evil.example.com/user/repos").is_err());
        assert!(resolve_cursor(base, "https://api.github.com.evil.example.com/x").is_err());
    }
}
