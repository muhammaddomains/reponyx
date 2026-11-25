//! Shared data-directory resolution: **one** function, used by the
//! desktop, the background agent, and every tool that opens the
//! database, so all three processes open the same database file
//! (audit DB-M3: the desktop, the agent, and the desktop's agent
//! module each had their own copy of these rules, and a drift
//! between any two silently produced two databases).
//!
//! Rules, in order: `$REPONYX_DATA_DIR`, else `%APPDATA%\RepoNyx`
//! on Windows, else `~/.local/share/RepoNyx`, else a
//! `.reponyx-data` fallback next to the working directory.

use std::path::{Path, PathBuf};

/// The RepoNyx data directory.
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
    resolve_data_dir(std::env::var_os("HOME"))
}

/// Pure fallback resolution, tested without touching the
/// environment: edition 2024 makes `std::env::set_var` unsafe,
/// and Semgrep flags every `unsafe` block, so the testable logic
/// lives here and the env reads stay in [`data_dir`].
fn resolve_data_dir(home: Option<std::ffi::OsString>) -> PathBuf {
    if let Some(home) = home {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("RepoNyx");
    }
    PathBuf::from(".reponyx-data")
}

/// `<data_dir>/data/reponyx.sqlite` — the one database file.
pub fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("data").join("reponyx.sqlite")
}

/// `<data_dir>/logs` — the log directory.
pub fn log_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_and_log_paths_nest_under_the_data_dir() {
        let data = PathBuf::from("/tmp/reponyx-test");
        assert_eq!(
            db_path(&data),
            PathBuf::from("/tmp/reponyx-test/data/reponyx.sqlite")
        );
        assert_eq!(log_dir(&data), PathBuf::from("/tmp/reponyx-test/logs"));
    }

    #[test]
    fn data_dir_falls_back_to_home_and_then_to_a_relative_dir() {
        assert_eq!(
            resolve_data_dir(Some("/home/tester".into())),
            PathBuf::from("/home/tester/.local/share/RepoNyx")
        );
        assert_eq!(resolve_data_dir(None), PathBuf::from(".reponyx-data"));
    }
}
