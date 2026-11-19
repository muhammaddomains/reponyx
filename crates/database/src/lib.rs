//! SQLite persistence (spec §14).
//!
//! - Migrations live in `migrations/sqlite/` and are embedded at compile
//!   time; they run on every open, so an old database upgrades itself.
//! - WAL mode + foreign keys on every connection.
//! - All timestamps are UTC (`TEXT` ISO-8601 via `chrono`).
//! - **No authentication secrets are stored in any table** — credentials
//!   live in the OS credential store (ADR-008).

pub mod counts;
pub mod pool;

pub use counts::dashboard_counts;
pub use pool::{Database, open_database};
