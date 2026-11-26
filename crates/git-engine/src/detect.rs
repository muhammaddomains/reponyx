//! Git executable detection and version validation.
//!
//! Minimum supported Git: **2.40.0** (spec §9). Detection searches PATH
//! (and `GIT_EXECUTABLE` if set) and parses `git --version`.

use reponyx_core::{ErrorCode, GitStatus, LfsStatus, RepoNyxError};
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

    let mut cmd = tokio::process::Command::new(&path);
    crate::process::hide_console(&mut cmd);
    let output = cmd.arg("--version").output().await.map_err(|e| {
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

/// Git LFS detection (Phase 3: detection only — LFS object sync
/// itself is Phase 9).
#[derive(Debug, Clone)]
pub struct LfsInfo {
    pub path: PathBuf,
    pub version: (u64, u64, u64),
    pub version_string: String,
}

/// Parse `git-lfs version 3.5.1` (from `git-lfs version`) or
/// `git-lfs/3.5.1 (GitHub; linux amd64; go 1.21.0)` (from
/// `git lfs version`).
pub fn parse_lfs_version(output: &str) -> Option<(u64, u64, u64)> {
    let trimmed = output.trim();
    let numeric_part = trimmed
        .strip_prefix("git-lfs version ")
        .or_else(|| trimmed.strip_prefix("git-lfs/"))?;
    let numeric: String = numeric_part
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut it = numeric.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// Locate `git-lfs` on PATH and validate its version. Returns
/// `GitLfsFailed` when no executable is found or the version cannot
/// be parsed.
pub async fn detect_lfs() -> Result<LfsInfo, RepoNyxError> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    detect_lfs_with_path(&path_var).await
}

/// [`detect_lfs`] with an explicit PATH value (test seam: tests pass
/// a temp dir holding a fake `git-lfs` script, or an empty string).
pub async fn detect_lfs_with_path(search_path: &str) -> Result<LfsInfo, RepoNyxError> {
    let exe = if cfg!(windows) {
        "git-lfs.exe"
    } else {
        "git-lfs"
    };
    let path = std::env::split_paths(search_path)
        .map(|dir| dir.join(exe))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            RepoNyxError::new(
                ErrorCode::GitLfsFailed,
                "git-lfs executable not found on PATH",
            )
        })?;

    let mut cmd = tokio::process::Command::new(&path);
    crate::process::hide_console(&mut cmd);
    let output = cmd.arg("version").output().await.map_err(|e| {
        RepoNyxError::new(ErrorCode::GitLfsFailed, "Failed to run git-lfs version")
            .with_details(e.to_string())
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = parse_lfs_version(&stdout).ok_or_else(|| {
        RepoNyxError::new(
            ErrorCode::GitLfsFailed,
            "Could not parse git-lfs version output",
        )
        .with_details(stdout.trim().to_string())
    })?;
    Ok(LfsInfo {
        path,
        version,
        version_string: stdout.trim().to_string(),
    })
}

/// Non-failing LFS status snapshot for the UI.
pub async fn lfs_status() -> LfsStatus {
    match detect_lfs().await {
        Ok(info) => LfsStatus {
            installed: true,
            path: Some(info.path.to_string_lossy().into_owned()),
            version: Some(info.version_string),
            error: None,
        },
        Err(e) => LfsStatus {
            installed: false,
            path: None,
            version: None,
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

    #[test]
    fn parses_lfs_versions() {
        assert_eq!(parse_lfs_version("git-lfs version 3.5.1"), Some((3, 5, 1)));
        assert_eq!(
            parse_lfs_version("git-lfs/3.5.1 (GitHub; linux amd64; go 1.21.0)"),
            Some((3, 5, 1))
        );
        assert_eq!(parse_lfs_version("git-lfs version 3.4"), Some((3, 4, 0)));
        assert_eq!(parse_lfs_version("not lfs"), None);
        assert_eq!(parse_lfs_version(""), None);
    }

    #[tokio::test]
    async fn lfs_absent_when_path_is_empty() {
        // This VM has no git-lfs; an empty search path must report it.
        let err = detect_lfs_with_path("").await.unwrap_err();
        assert_eq!(err.code, ErrorCode::GitLfsFailed);
    }

    #[tokio::test]
    async fn lfs_detected_via_fake_shim_on_path() {
        let dir = tempfile::tempdir().unwrap();
        let shim = dir.path().join("git-lfs");
        std::fs::write(&shim, "#!/bin/sh\necho 'git-lfs version 3.5.1'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // This VM's /tmp (tmpfs) intermittently returns ETXTBSY
        // ("Text file busy") when a freshly written script is
        // executed immediately under parallel load — reproduced
        // independently at ~5% with a write/chmod/exec stress loop,
        // while exec of an unchanged file never flakes. Retry a few
        // times: the shim itself is fine, the filesystem is racing.
        let mut last_err = None;
        for _ in 0..5 {
            match detect_lfs_with_path(dir.path().to_str().unwrap()).await {
                Ok(info) => {
                    assert_eq!(info.version, (3, 5, 1));
                    assert_eq!(info.path, shim);
                    return;
                }
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            }
        }
        panic!("git-lfs shim not detected after retries: {:?}", last_err);
    }
}
