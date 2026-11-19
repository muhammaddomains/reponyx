//! Git engine (spec §9): the independent, reusable Git CLI layer.
//!
//! Safety rules, enforced here and nowhere else:
//! - Git is invoked via argument arrays only — never a shell, never
//!   string concatenation of user-controlled input.
//! - Tokens are never placed in clone URLs or process arguments.
//!   Credentials are injected via a short-lived credential helper /
//!   controlled environment (Phase 3+); process output is redacted.
//! - Every operation has a timeout and supports cancellation.
//! - One operation at a time per local path (see `locks`, Phase 3).

pub mod commands;
pub mod detect;
pub mod paths;
pub mod process;

pub use commands::{GitCommand, build_clone_mirror, build_fetch, build_ls_remote};
pub use detect::{GitInfo, detect_git, parse_git_version};
pub use paths::{RepositoryLayout, validate_path_component, validate_repository_path};
