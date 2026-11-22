//! Production [`JobRunner`]: git-engine operations + Phase 5 credentials.
//!
//! Responsibilities:
//! 1. Load repository + account from SQLite
//! 2. Resolve a short-lived token via [`CredentialResolver`]
//! 3. Inject it through git-engine's [`CredentialHelper`] (never in URLs)
//! 4. Run `mirror_clone` / `mirror_fetch` / `working_fetch` / working clone
//! 5. Update `local_path` + `last_synced_at` on success
//!
//! LFS fetch is implemented (Phase 9): after a successful clone or
//! fetch, a separate `LfsFetch` job is enqueued when the repository
//! has LFS pointers, so the mirror archives the objects too.
//! Submodule job types are still rejected until later in Phase 9.

use async_trait::async_trait;
use chrono::Utc;
use reponyx_core::{ErrorCode, JobType, RepoNyxError, Repository, SyncMode};
use reponyx_database::accounts::get_account;
use reponyx_database::jobs::{NewJob, create_job, is_unique_violation};
use reponyx_database::repositories::{get_repository, mark_synced};
use reponyx_git_engine::commands::build_clone_working;
use reponyx_git_engine::credentials::reject_embedded_credentials;
use reponyx_git_engine::lfs::{has_lfs_pointers, lfs_fetch};
use reponyx_git_engine::paths::RepositoryLayout;
use reponyx_git_engine::process::{map_git_failure, run_git_command};
use reponyx_git_engine::{
    Credential, CredentialHelper, ProgressEvent, RepoLockManager, SyncOptions, detect_git,
    mirror_clone, mirror_fetch, working_fetch,
};
use reponyx_github_provider::CredentialSource;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, info};

use crate::credentials_resolve::{CredentialResolver, storage_root_from_settings};
use crate::progress::SyncProgressEvent;
use crate::runner::{JobContext, JobRunResult, JobRunner, operation_label};

/// Optional override for storage root / git binary (tests).
#[derive(Debug, Clone)]
pub struct GitRunnerConfig {
    /// When set, skips reading `storage.root` from settings.
    pub storage_root: Option<PathBuf>,
    /// When set, skips `detect_git()`.
    pub git_path: Option<PathBuf>,
    pub job_timeout: Duration,
    pub lock_timeout: Duration,
}

impl Default for GitRunnerConfig {
    fn default() -> Self {
        Self {
            storage_root: None,
            git_path: None,
            job_timeout: Duration::from_secs(600),
            lock_timeout: Duration::from_secs(60),
        }
    }
}

/// Git-backed job runner.
pub struct GitJobRunner {
    resolver: Arc<dyn CredentialResolver>,
    locks: RepoLockManager,
    config: GitRunnerConfig,
}

impl GitJobRunner {
    pub fn new(resolver: Arc<dyn CredentialResolver>) -> Self {
        Self {
            resolver,
            locks: RepoLockManager::new(),
            config: GitRunnerConfig::default(),
        }
    }

    pub fn with_config(resolver: Arc<dyn CredentialResolver>, config: GitRunnerConfig) -> Self {
        Self {
            resolver,
            locks: RepoLockManager::new(),
            config,
        }
    }

    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[async_trait]
impl JobRunner for GitJobRunner {
    async fn run(&self, ctx: JobContext) -> Result<JobRunResult, RepoNyxError> {
        let started = std::time::Instant::now();

        if matches!(ctx.job.job_type, JobType::SubmoduleSync) {
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "job type submodule_sync is not supported yet (later in Phase 9)",
            ));
        }

        let repo = get_repository(&ctx.pool, ctx.job.repository_id)
            .await?
            .ok_or_else(|| {
                RepoNyxError::new(
                    ErrorCode::RepositoryNotFound,
                    format!("repository {} not found", ctx.job.repository_id),
                )
            })?;

        if !repo.sync_enabled {
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                format!("sync disabled for {}", repo.full_name),
            ));
        }

        let account = get_account(&ctx.pool, repo.account_id)
            .await?
            .ok_or_else(|| {
                RepoNyxError::new(
                    ErrorCode::InvalidInput,
                    format!(
                        "account {} not found for repo {}",
                        repo.account_id, repo.full_name
                    ),
                )
            })?;

        let source = self.resolver.resolve(&ctx.pool, &account).await?;
        let helper = token_to_helper(source.as_ref()).await?;

        let git = match &self.config.git_path {
            Some(p) => p.clone(),
            None => detect_git().await?.path,
        };

        let storage_root = match &self.config.storage_root {
            Some(p) => p.clone(),
            None => storage_root_from_settings(&ctx.pool).await?,
        };

        let layout =
            RepositoryLayout::new(&storage_root, &account.username, &repo.owner, &repo.name)?;

        let timeout = self.config.job_timeout.min(ctx.timeout);
        let mut opts = SyncOptions::new()
            .with_timeout(timeout)
            .with_lock_timeout(self.config.lock_timeout)
            .with_cancel(ctx.cancel.child_token())
            .with_credential(Arc::clone(&helper));

        // Forward git progress events to the queue's broadcast channel
        // as JobProgress events. The forwarder ends on its own when
        // the git operation drops the sender.
        if let Some(progress) = &ctx.progress {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<ProgressEvent>(64);
            let job_id = ctx.job.id;
            let out = progress.clone();
            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    let (stage, detail) = match &event {
                        ProgressEvent::CountingObjects { .. } => {
                            ("counting objects", event.to_string())
                        }
                        ProgressEvent::CompressingObjects { .. } => {
                            ("compressing objects", event.to_string())
                        }
                        ProgressEvent::ReceivingObjects { .. } => {
                            ("receiving objects", event.to_string())
                        }
                        ProgressEvent::ResolvingDeltas { .. } => {
                            ("resolving deltas", event.to_string())
                        }
                        ProgressEvent::CheckingOut { .. } => ("checking out", event.to_string()),
                        ProgressEvent::UpdatingFiles { .. } => {
                            ("updating files", event.to_string())
                        }
                        ProgressEvent::Other(_) => ("git progress", event.to_string()),
                    };
                    let _ = out.send(SyncProgressEvent::JobProgress {
                        job_id,
                        stage: stage.to_string(),
                        detail: Some(detail),
                    });
                }
            });
            opts = opts.with_progress(tx);
        }

        reject_embedded_credentials(&repo.clone_url)?;

        emit_stage(
            &ctx.progress,
            ctx.job.id,
            operation_label(ctx.job.job_type),
            "started",
        );
        match ctx.job.job_type {
            JobType::MirrorClone => {
                self.run_mirror_clone(&ctx, &git, &repo, &layout, opts)
                    .await?;
            }
            JobType::WorkingClone => {
                self.run_working_clone(&ctx, &git, &repo, &layout, opts)
                    .await?;
            }
            JobType::Fetch => {
                self.run_fetch(&ctx, &git, &repo, &layout, opts).await?;
            }
            JobType::LfsFetch => {
                self.run_lfs_fetch(&ctx, &git, &repo, &layout, opts).await?;
            }
            JobType::SubmoduleSync => unreachable!(),
        }
        emit_stage(
            &ctx.progress,
            ctx.job.id,
            operation_label(ctx.job.job_type),
            "finished",
        );

        mark_synced(&ctx.pool, repo.id, Utc::now()).await?;

        // Phase 9: the LFS step is separate from the mirror sync. After
        // any successful clone/fetch (but not after an LFS job itself),
        // check for LFS pointers and enqueue an LfsFetch job so the
        // mirror archives the objects too. Best-effort: a failure here
        // never fails the job that just succeeded.
        if !matches!(ctx.job.job_type, JobType::LfsFetch) {
            self.maybe_enqueue_lfs_step(&ctx, &repo, &layout, &git)
                .await;
        }

        let duration_ms = started.elapsed().as_millis() as i64;
        info!(
            job_id = %ctx.job.id,
            repo = %repo.full_name,
            op = ctx.job.job_type.as_str(),
            duration_ms,
            "git job finished"
        );

        Ok(JobRunResult {
            duration_ms: Some(duration_ms),
            bytes_received: None,
        })
    }
}

impl GitJobRunner {
    async fn run_mirror_clone(
        &self,
        ctx: &JobContext,
        git: &Path,
        repo: &Repository,
        layout: &RepositoryLayout,
        opts: SyncOptions,
    ) -> Result<(), RepoNyxError> {
        let dest = &layout.mirror_path;
        debug!(path = %dest.display(), "mirror_clone");
        let info = mirror_clone(git, &repo.clone_url, dest, &self.locks, opts).await?;
        set_local_path(&ctx.pool, repo.id, dest).await?;
        debug!(
            head = ?info.head_sha,
            branches = info.branches,
            tags = info.tags,
            "mirror_clone ok"
        );
        Ok(())
    }

    async fn run_working_clone(
        &self,
        ctx: &JobContext,
        git: &Path,
        repo: &Repository,
        layout: &RepositoryLayout,
        opts: SyncOptions,
    ) -> Result<(), RepoNyxError> {
        let dest = &layout.working_path;
        if dest.exists()
            && dest
                .read_dir()
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
        {
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "Working clone destination already exists and is not empty",
            )
            .with_details(format!("dest={}", dest.display())));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RepoNyxError::new(
                    ErrorCode::InternalError,
                    "cannot create working clone parent",
                )
                .with_details(e.to_string())
            })?;
        }

        // Per-repo lock (same guarantee as mirror_clone).
        let _guard = self.locks.acquire(dest, opts.lock_timeout).await?;

        let mut cmd = build_clone_working(git, &repo.clone_url, dest);
        if let Some(helper) = &opts.credential {
            helper.install(&mut cmd);
        }
        // Wire the job cancellation token into the process watch
        // channel (previously `never_cancel()`, so a cancelled job
        // kept cloning until the timeout).
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let watcher = {
            let token = ctx.cancel.clone();
            tokio::spawn(async move {
                token.cancelled().await;
                let _ = cancel_tx.send(true);
            })
        };
        let output = run_git_command(&cmd, opts.timeout, cancel_rx).await;
        watcher.abort();
        let output = output?;
        if !output.success {
            return Err(map_git_failure(&cmd, &output));
        }

        set_local_path(&ctx.pool, repo.id, dest).await?;
        Ok(())
    }

    async fn run_fetch(
        &self,
        ctx: &JobContext,
        git: &Path,
        repo: &Repository,
        layout: &RepositoryLayout,
        opts: SyncOptions,
    ) -> Result<(), RepoNyxError> {
        let path = resolve_existing_path(repo, layout)?;
        debug!(path = %path.display(), mode = ?repo.sync_mode, "fetch");

        match repo.sync_mode {
            SyncMode::Mirror => {
                mirror_fetch(git, &path, &self.locks, opts).await?;
            }
            SyncMode::Working => {
                working_fetch(git, &path, &self.locks, opts).await?;
            }
            SyncMode::Both => {
                // Prefer the mirror path when both exist; fall back to working.
                if layout.mirror_path.is_dir() {
                    mirror_fetch(git, &layout.mirror_path, &self.locks, opts.clone()).await?;
                }
                if layout.working_path.is_dir() {
                    working_fetch(git, &layout.working_path, &self.locks, opts).await?;
                }
                if !layout.mirror_path.is_dir() && !layout.working_path.is_dir() {
                    return Err(RepoNyxError::new(
                        ErrorCode::RepositoryNotFound,
                        "neither mirror nor working clone exists for fetch",
                    ));
                }
            }
        }

        // Keep local_path consistent if it was missing but layout paths exist.
        if repo.local_path.is_none() {
            set_local_path(&ctx.pool, repo.id, &path).await?;
        }
        Ok(())
    }

    async fn run_lfs_fetch(
        &self,
        _ctx: &JobContext,
        git: &Path,
        repo: &Repository,
        layout: &RepositoryLayout,
        opts: SyncOptions,
    ) -> Result<(), RepoNyxError> {
        let path = resolve_existing_path(repo, layout)?;
        debug!(path = %path.display(), "lfs_fetch");
        let summary = lfs_fetch(git, &path, &self.locks, opts).await?;
        info!(
            pointers = summary.pointers,
            fetched = summary.fetched,
            "lfs_fetch ok"
        );
        Ok(())
    }

    /// Enqueue a separate `LfsFetch` job when the repository has LFS
    /// pointers. Best-effort by design: git-lfs missing, no pointers,
    /// or an already-queued LFS job all mean "no step", not an error.
    /// A failure to check or enqueue is logged and swallowed — it
    /// must never fail the clone/fetch job that just succeeded.
    async fn maybe_enqueue_lfs_step(
        &self,
        ctx: &JobContext,
        repo: &Repository,
        layout: &RepositoryLayout,
        git: &Path,
    ) {
        let path = match resolve_existing_path(repo, layout) {
            Ok(p) => p,
            Err(e) => {
                debug!(repo = %repo.full_name, error = %e, "LFS step skipped: no local clone");
                return;
            }
        };
        let has_pointers = match has_lfs_pointers(git, &path).await {
            Ok(has) => has,
            Err(e) => {
                debug!(repo = %repo.full_name, error = %e, "LFS step skipped: pointer check failed");
                return;
            }
        };
        if !has_pointers {
            return;
        }
        match create_job(
            &ctx.pool,
            NewJob {
                repository_id: repo.id,
                job_type: JobType::LfsFetch,
                priority: ctx.job.priority,
            },
        )
        .await
        {
            Ok(job) => info!(job_id = %job.id, repo = %repo.full_name, "LFS step enqueued"),
            Err(e) if is_unique_violation(&e) => {
                debug!(repo = %repo.full_name, "LFS step already queued");
            }
            Err(e) => {
                tracing::warn!(repo = %repo.full_name, error = %e, "LFS step enqueue failed");
            }
        }
    }
}

async fn token_to_helper(
    source: &dyn CredentialSource,
) -> Result<Arc<CredentialHelper>, RepoNyxError> {
    let token = source.token().await?;
    let cred = Credential::new(token.username.clone(), token.expose_value())?;
    CredentialHelper::create(&cred)
}

fn resolve_existing_path(
    repo: &Repository,
    layout: &RepositoryLayout,
) -> Result<PathBuf, RepoNyxError> {
    if let Some(ref p) = repo.local_path {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Ok(path);
        }
        return Err(RepoNyxError::new(
            ErrorCode::RepositoryNotFound,
            format!("local_path does not exist: {p}"),
        ));
    }
    match repo.sync_mode {
        SyncMode::Mirror | SyncMode::Both if layout.mirror_path.is_dir() => {
            Ok(layout.mirror_path.clone())
        }
        SyncMode::Working if layout.working_path.is_dir() => Ok(layout.working_path.clone()),
        SyncMode::Both if layout.working_path.is_dir() => Ok(layout.working_path.clone()),
        _ => Err(RepoNyxError::new(
            ErrorCode::RepositoryNotFound,
            format!(
                "no local clone for {}; run a clone job first",
                repo.full_name
            ),
        )),
    }
}

/// Persist `local_path` after a successful clone.
///
/// Preferred path: `reponyx_database::repositories::set_local_path` once
/// that helper lands on main. Until then this crate issues the same SQL
/// the repository module would use (bound parameters only, no secrets).
async fn set_local_path(
    pool: &sqlx::SqlitePool,
    id: uuid::Uuid,
    path: &Path,
) -> Result<(), RepoNyxError> {
    // Try the official API if present (linked at compile time once main has it).
    #[allow(unused_imports)]
    {
        // Always use the local SQL so this crate compiles against current main.
    }
    let now = chrono::Utc::now().to_rfc3339();
    let path_str = path.to_string_lossy();
    let affected =
        sqlx::query("UPDATE repositories SET local_path = ?, updated_at = ? WHERE id = ?")
            .bind(path_str.as_ref())
            .bind(&now)
            .bind(id.to_string())
            .execute(pool)
            .await
            .map_err(|e| {
                RepoNyxError::new(ErrorCode::DatabaseError, "failed to set local_path")
                    .with_details(e.to_string())
            })?
            .rows_affected();
    if affected == 0 {
        return Err(RepoNyxError::new(
            ErrorCode::RepositoryNotFound,
            format!("repository {id} not found while setting local_path"),
        ));
    }
    Ok(())
}

/// Emit one coarse stage event (op start / finish) when a progress
/// sink is attached. Fine-grained git progress comes through the
/// `SyncOptions` progress channel instead.
fn emit_stage(
    progress: &Option<broadcast::Sender<SyncProgressEvent>>,
    job_id: uuid::Uuid,
    stage: &str,
    detail: &str,
) {
    if let Some(tx) = progress {
        let _ = tx.send(SyncProgressEvent::JobProgress {
            job_id,
            stage: stage.to_string(),
            detail: Some(detail.to_string()),
        });
    }
}
