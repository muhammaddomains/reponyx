//! Git engine (spec §9): the independent, reusable Git CLI layer.
//!
//! Safety rules, enforced here and nowhere else:
//! - Git is invoked via argument arrays only — never a shell, never
//!   string concatenation of user-controlled input.
//! - Tokens are never placed in clone URLs or process arguments.
//!   Credentials are injected via a short-lived credential helper
//!   ([`credentials`]) that serves them from the child process
//!   environment; process output is redacted.
//! - Every long operation has a timeout and supports cancellation
//!   ([`options`]).
//! - One operation at a time per local path ([`locks`]).
//! - Working clones are never reset, checked out, or cleaned by this
//!   crate — fetch only ([`working`]).
//! - Diagnosis never deletes or repairs anything ([`health`]).

pub mod commands;
pub mod credentials;
pub mod detect;
pub mod health;
pub mod locks;
pub mod mirror;
pub mod options;
pub mod paths;
pub mod process;
pub mod progress;
pub mod refs;
pub mod storage;
pub mod working;

pub use commands::{GitCommand, build_clone_mirror, build_fetch, build_ls_remote};
pub use credentials::{Credential, CredentialHelper, reject_embedded_credentials};
pub use detect::{GitInfo, LfsInfo, detect_git, detect_lfs, lfs_status, parse_git_version};
pub use health::{HealthStatus, RepositoryDiagnosis, verify_repository};
pub use locks::{RepoLockGuard, RepoLockManager};
pub use mirror::{FetchSummary, MirrorInfo, mirror_clone, mirror_fetch};
pub use options::{DEFAULT_LOCK_TIMEOUT, DEFAULT_TIMEOUT, SyncOptions};
pub use paths::{RepositoryLayout, validate_path_component, validate_repository_path};
pub use progress::{ProgressEvent, parse_progress_line};
pub use refs::{BranchInfo, RefKind, TagInfo, head_sha, list_branches, list_tags};
pub use working::{
    Divergence, WorkingState, WorkingStatus, divergence, working_fetch, working_status,
};
