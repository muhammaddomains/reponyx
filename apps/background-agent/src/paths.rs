//! Shared data-directory resolution (must match the desktop app).
//!
//! Thin delegates to `reponyx_database::paths` — the rules live in
//! exactly one place (audit DB-M3), so the agent and the desktop
//! can never disagree about where the database is.

use std::path::{Path, PathBuf};

pub fn data_dir() -> PathBuf {
    reponyx_database::data_dir()
}

pub fn db_path(data_dir: &Path) -> PathBuf {
    reponyx_database::db_path(data_dir)
}

pub fn log_dir(data_dir: &Path) -> PathBuf {
    reponyx_database::log_dir(data_dir)
}
