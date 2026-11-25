//! SQLite persistence (spec §14).
//!
//! - Migrations live in `migrations/sqlite/` and are embedded at compile
//!   time; they run on every open, so an old database upgrades itself.
//! - WAL mode + foreign keys on every connection.
//! - All timestamps are UTC (`TEXT` ISO-8601 via `chrono`).
//! - **No authentication secrets are stored in any table** — credentials
//!   live in the OS credential store (ADR-008).
//! - One repository module per table (Phase 2); all SQL uses bound
//!   parameters only.

pub mod accounts;
pub mod audit;
pub mod config;
pub mod counts;
pub mod devices;
pub mod diagnostics;
pub mod history;
pub mod installations;
pub mod jobs;
pub mod paths;
pub mod pool;
pub mod repositories;
pub mod schedules;
pub mod settings;

mod common;

pub use config::{
    ConfigFile, ConfigRepository, ConfigSchedule, ConfigSetting, ImportReport, SkippedRef,
    apply_config_import, build_config_export, parse_config_file, validate_setting_value,
};
pub use counts::dashboard_counts;
pub use diagnostics::{
    DiagnosticsCounts, backup_database, diagnostics_counts, is_safe_for_diagnostics,
    validate_backup_file,
};
pub use paths::{data_dir, db_path, log_dir};
pub use pool::{Database, open_database};
