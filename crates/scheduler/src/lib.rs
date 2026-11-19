//! Scheduler crate (Phase 7, scheduler part only).
//!
//! What this crate does: schedule math. It computes the next run time
//! of a [`Schedule`](reponyx_core::Schedule), finds due schedules, and
//! creates sync-job rows for them. It works off the Phase 2
//! schedules/jobs repository functions in `reponyx-database` — it does
//! not duplicate their logic and it never runs raw SQL itself.
//!
//! What this crate does NOT do (hard boundary): it never executes a
//! sync. It does not spawn git, call a provider, run a job, retry
//! anything, or manage a worker pool. Creating a job row is where it
//! stops — running the job is the Phase 6 sync queue's job, and this
//! crate deliberately has no dependency that could run one
//! (`reponyx-core` + `reponyx-database` only; see `cargo tree -p
//! reponyx-scheduler`). The background agent, IPC, and Task Scheduler
//! registration are also out of scope here — they are the rest of
//! Phase 7 and are blocked on Phase 6.
//!
//! Time model: all stored times are UTC. A schedule's `timezone` is
//! an IANA name (e.g. `UTC`, `America/Phoenix`); daily/weekly run
//! times are wall-clock times in that zone. DST rules (see
//! [`next_run`]): a scheduled local time that does not exist
//! (spring-forward gap) fires at the first valid time after the gap;
//! a scheduled local time that occurs twice (fall-back ambiguity)
//! fires once, at the first occurrence.

pub mod cadence;
pub mod due;
pub mod next_run;
pub mod timezone;

pub use cadence::{Cadence, CadenceKind, TimeOfDay, TimeWindow, parse_cadence};
pub use due::{
    EffectiveNextRuns, MaterializeReport, ScheduleMaterializeError, effective_next_runs,
    materialize_due,
};
pub use next_run::{next_run, next_runs};
pub use timezone::{display_time, parse_timezone};
