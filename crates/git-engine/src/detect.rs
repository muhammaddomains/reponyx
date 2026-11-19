//! Git executable detection and version validation.
//!
//! Minimum supported Git: **2.40.0** (spec §9). Detection searches PATH
//! (and `GIT_EXECUTABLE` if set) and parses `git --version`.

use reponyx_core::{ErrorCode, GitStatus, RepoNyxError};
use std::path::PathBuf;

/// Minimum supported Git version.
pub const MIN_GIT_VERSION: (u64, u64, u64) = (2, 40, 0);

#[derive(Debug, Clone)]
pub struct GitInfo {
    pub path: PathBuf,
    pub version: (u64, u64, u64),
    pub version_string: String,
}

impl GitInfo {
    pub fn is_supported(&self) -> bool {
        self.version >= MIN_GIT_VERSION
    }
}

/// Parse `git version 2.43.0` / `git version 2.43.0.windows.1` etc.
pub fn parse_git_version(output: &str) -> Option<(u64, u64, u64)> {
    let version_part = output.trim().strip_prefix("git version ")?;
    let numeric: String = version_part
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut it = numeric.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// Locate Git and validate its version. Returns `GitNotFound` when no
/// executable is found, `GitVersionUnsupported` when it is too old.
pub async fn detect_git() -> Result<GitInfo, RepoNyxError> {
    let candidates: Vec<PathBuf> = match std::env::var("GIT_EXECUTABLE") {
        Ok(p) if !p.is_empty() => vec![PathBuf::from(p)],
        _ => which_git(),
    };
    let path = candidates.into_iter().next().ok_or_else(|| {
        RepoNyxError::new(ErrorCode::GitNotFound, "Git executable not found on PATH")
    })?;

    let output = tokio::process::Command::new(&path)
        .arg("--version")
        .output()
        .await
        .map_err(|e| {
            RepoNyxError::new(ErrorCode::GitNotFound, "Failed to run git --version")
                .with_details(e.to_string())
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = parse_git_version(&stdout).ok_or_else(|| {
        RepoNyxError::new(
            ErrorCode::GitVersionUnsupported,
            "Could not parse git version output",
        )
        .with_details(stdout.trim().to_string())
    })?;
    let info = GitInfo {
        path,
        version,
        version_string: stdout.trim().to_string(),
    };
    if !info.is_supported() {
        return Err(RepoNyxError::new(
            ErrorCode::GitVersionUnsupported,
            format!(
                "Git {}.{}.{} is too old; RepoNyx requires >= {}.{}.{}",
                version.0,
                version.1,
                version.2,
                MIN_GIT_VERSION.0,
                MIN_GIT_VERSION.1,
                MIN_GIT_VERSION.2
            ),
        ));
    }
    Ok(info)
}

/// Search PATH for `git` / `git.exe` without invoking a shell.
fn which_git() -> Vec<PathBuf> {
    let Ok(path_var) = std::env::var("PATH") else {
        return Vec::new();
    };
    let exe = if cfg!(windows) { "git.exe" } else { "git" };
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(exe))
        .filter(|p| p.is_file())
        .collect()
}

/// Non-failing status snapshot for the UI: reports availability instead
/// of returning an error.
pub async fn git_status() -> GitStatus {
    match detect_git().await {
        Ok(info) => GitStatus {
            available: true,
            path: Some(info.path.to_string_lossy().into_owned()),
            version: Some(info.version_string),
            supported: true,
            error: None,
        },
        Err(e) => GitStatus {
            available: false,
            path: None,
            version: None,
            supported: false,
            error: Some(format!("{}: {}", e.code, e.message)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_version() {
        assert_eq!(parse_git_version("git version 2.43.0"), Some((2, 43, 0)));
    }

    #[test]
    fn parses_windows_version_suffix() {
        assert_eq!(
            parse_git_version("git version 2.43.0.windows.1"),
            Some((2, 43, 0))
        );
    }

    #[test]
    fn parses_partial_version() {
        assert_eq!(parse_git_version("git version 2.40"), Some((2, 40, 0)));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_git_version("not git"), None);
        assert_eq!(parse_git_version(""), None);
    }

    #[test]
    fn version_support_threshold() {
        let ok = GitInfo {
            path: PathBuf::from("git"),
            version: (2, 40, 0),
            version_string: "git version 2.40.0".into(),
        };
        assert!(ok.is_supported());
        let old = GitInfo {
            version: (2, 39, 9),
            ..ok.clone()
        };
        assert!(!old.is_supported());
    }

    #[tokio::test]
    async fn detects_system_git() {
        // The dev/CI machine has Git; this asserts detection works there.
        let info = detect_git().await.expect("git should be installed");
        assert!(info.is_supported());
    }
}
