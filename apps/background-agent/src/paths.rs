//! Shared data-directory resolution (must match the desktop app).

use std::path::PathBuf;

/// Same rules as `apps/desktop/src-tauri` so both processes share one DB.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("REPONYX_DATA_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("RepoNyx");
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("RepoNyx");
    }
    PathBuf::from(".reponyx-data")
}

pub fn db_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("data").join("reponyx.sqlite")
}

pub fn log_dir(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("logs")
}
