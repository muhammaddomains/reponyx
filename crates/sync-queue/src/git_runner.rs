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
//! Submodule sync is implemented the same way: a separate
//! `SubmoduleSync` job fetches submodule commits into the mirror
//! and checks out submodules in a working clone.

use async_trait::async_trait;
use chrono::Utc;
use reponyx_core::{ErrorCode, JobType, RepoNyxError, Repository, SyncMode};
use reponyx_database::accounts::get_account;
use reponyx_database::jobs::{NewJob, create_job, is_unique_violation};
use reponyx_database::repositories::{get_repository, mark_synced, set_local_path};
use reponyx_git_engine::commands::build_clone_working;
use reponyx_git_engine::credentials::reject_embedded_credentials;
use reponyx_git_engine::lfs::{has_lfs_pointers, lfs_fetch};
use reponyx_git_engine::paths::RepositoryLayout;
use reponyx_git_engine::process::{map_git_failure, run_git_command};
use reponyx_git_engine::submodule::{
    SubmoduleFailure, has_submodules, sync_submodules_mirror, sync_submodules_working,
};
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

        let mut repo = get_repository(&ctx.pool, ctx.job.repository_id)
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
                    let (stage, detail, percent) = match &event {
                        ProgressEvent::CountingObjects { received, total } => (
                            "counting objects",
                            event.to_string(),
                            total.filter(|t| *t > 0).map(|t| {
                                received.saturating_mul(100).saturating_div(t).min(100) as u8
                            }),
                        ),
                        ProgressEvent::CompressingObjects { percent } => {
                            ("compressing objects", event.to_string(), Some(*percent))
                        }
                        ProgressEvent::ReceivingObjects { percent, .. } => {
                            ("receiving objects", event.to_string(), Some(*percent))
                        }
                        ProgressEvent::ResolvingDeltas { percent } => {
                            ("resolving deltas", event.to_string(), Some(*percent))
                        }
                        ProgressEvent::CheckingOut { percent } => {
                            ("checking out", event.to_string(), Some(*percent))
                        }
                        ProgressEvent::UpdatingFiles { percent } => {
                            ("updating files", event.to_string(), Some(*percent))
                        }
                        ProgressEvent::Other(_) => ("git progress", event.to_string(), None),
                    };
                    let _ = out.send(SyncProgressEvent::JobProgress {
                        job_id,
                        stage: stage.to_string(),
                        detail: Some(detail),
                        percent,
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
        // A fetch works on the archive that already exists. When
        // the layout collapsed (account == owner), sides cloned
        // under the old doubled layout must move first — otherwise
        // a Both repo fails the fetch ("neither mirror nor working
        // clone exists") and a single-mode repo keeps fetching at
        // the old path forever. The in-memory row is patched too:
        // the fetch resolves its path from it, not from the DB.
        if ctx.job.job_type == JobType::Fetch
            && let Some(new_local_path) = self
                .relocate_collapsed_layout(&ctx, &repo, &layout, &storage_root, &account.username)
                .await?
        {
            repo.local_path = Some(new_local_path.to_string_lossy().into_owned());
        }
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
            JobType::SubmoduleSync => {
                self.run_submodule_sync(&ctx, &git, &repo, &layout, opts)
                    .await?;
            }
        }
        emit_stage(
            &ctx.progress,
            ctx.job.id,
            operation_label(ctx.job.job_type),
            "finished",
        );

        mark_synced(&ctx.pool, repo.id, Utc::now()).await?;

        // Phase 9: the LFS and submodule steps are separate from the
        // mirror sync. After any successful clone/fetch (but not
        // after an LFS/submodule job itself), check for LFS pointers
        // and submodules and enqueue the follow-up jobs so the
        // mirror archives the objects too. Best-effort: a failure
        // here never fails the job that just succeeded.
        if !matches!(ctx.job.job_type, JobType::LfsFetch | JobType::SubmoduleSync) {
            self.maybe_enqueue_lfs_step(&ctx, &repo, &layout, &git)
                .await;
            self.maybe_enqueue_submodule_step(&ctx, &repo, &layout, &git)
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
    /// Move archive sides from the pre-collapse doubled layout
    /// (`<account>/<owner>/<name>`) to the current layout, which
    /// omits the owner segment when the account and the owner are
    /// the same user. Only moves a side when the old path exists
    /// and the new path does not. Returns the new `local_path`
    /// when the row's `local_path` pointed at a moved side (the
    /// caller patches its in-memory row — the fetch resolves its
    /// path from it). The per-path locks for both the old and the
    /// new path are held during the move so a concurrent
    /// clone/fetch for the same repo cannot race it.
    async fn relocate_collapsed_layout(
        &self,
        ctx: &JobContext,
        repo: &Repository,
        layout: &RepositoryLayout,
        storage_root: &Path,
        account_username: &str,
    ) -> Result<Option<PathBuf>, RepoNyxError> {
        let legacy_mirror = storage_root
            .join("repositories")
            .join("github")
            .join(account_username)
            .join(&repo.owner)
            .join(format!("{}.git", repo.name));
        let legacy_working = storage_root
            .join("working-clones")
            .join(account_username)
            .join(&repo.owner)
            .join(&repo.name);
        let mut new_local_path = None;
        for (legacy, current) in [
            (legacy_mirror, layout.mirror_path.clone()),
            (legacy_working, layout.working_path.clone()),
        ] {
            if legacy == current || !legacy.is_dir() || current.exists() {
                continue;
            }
            let _legacy_guard = self
                .locks
                .acquire(&legacy, self.config.lock_timeout)
                .await?;
            let _current_guard = self
                .locks
                .acquire(&current, self.config.lock_timeout)
                .await?;
            // Re-check after acquiring: a concurrent job may have
            // moved or created the side while we waited.
            if !legacy.is_dir() || current.exists() {
                continue;
            }
            if let Some(parent) = current.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    RepoNyxError::new(
                        ErrorCode::InternalError,
                        "cannot create relocated archive parent",
                    )
                    .with_details(e.to_string())
                })?;
            }
            std::fs::rename(&legacy, &current).map_err(|e| {
                RepoNyxError::new(
                    ErrorCode::InternalError,
                    "cannot relocate archive to the collapsed layout",
                )
                .with_details(format!(
                    "{} -> {}: {e}",
                    legacy.display(),
                    current.display()
                ))
            })?;
            if repo.local_path.as_deref() == legacy.to_str() {
                set_local_path(&ctx.pool, repo.id, Some(&current)).await?;
                new_local_path = Some(current.clone());
            }
            debug!(
                from = %legacy.display(),
                to = %current.display(),
                "relocated archive to the collapsed layout"
            );
        }
        Ok(new_local_path)
    }

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
        set_local_path(&ctx.pool, repo.id, Some(dest)).await?;
        debug!(
            head = ?info.head_sha,
            branches = info.branches,
            tags = info.tags,
            "mirror_clone ok"
        );
        // Phase 9 (Both-mode completion): a Both repo is not done
        // with just the mirror — enqueue the working clone as its
        // own job (independent failure, visible in the queue).
        // The dedupe key makes a second enqueue a no-op.
        if repo.sync_mode == SyncMode::Both && !layout.working_path.is_dir() {
            self.enqueue_followup_job(ctx, repo, JobType::WorkingClone, "working-clone")
                .await;
        }
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

        set_local_path(&ctx.pool, repo.id, Some(dest)).await?;
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
                // Fetch every side that exists; a missing side is
                // not a fetch failure — its clone is enqueued as a
                // follow-up job (covers mirrors deleted externally
                // and Both repos created before this existed).
                let mirror_exists = layout.mirror_path.is_dir();
                let working_exists = layout.working_path.is_dir();
                if !mirror_exists && !working_exists {
                    return Err(RepoNyxError::new(
                        ErrorCode::RepositoryNotFound,
                        "neither mirror nor working clone exists for fetch",
                    ));
                }
                if mirror_exists {
                    mirror_fetch(git, &layout.mirror_path, &self.locks, opts.clone()).await?;
                } else {
                    self.enqueue_followup_job(ctx, repo, JobType::MirrorClone, "mirror-clone")
                        .await;
                }
                if working_exists {
                    working_fetch(git, &layout.working_path, &self.locks, opts).await?;
                } else {
                    self.enqueue_followup_job(ctx, repo, JobType::WorkingClone, "working-clone")
                        .await;
                }
            }
        }

        // Keep local_path consistent if it was missing but layout paths exist.
        if repo.local_path.is_none() {
            set_local_path(&ctx.pool, repo.id, Some(&path)).await?;
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
        // The mirror is the archive: for Mirror/Both repos the LFS
        // objects belong there first, even when local_path currently
        // points at the working clone (a Both repo's last clone).
        let path = match repo.sync_mode {
            SyncMode::Mirror | SyncMode::Both if layout.mirror_path.is_dir() => {
                layout.mirror_path.clone()
            }
            _ => resolve_existing_path(repo, layout)?,
        };
        debug!(path = %path.display(), "lfs_fetch");
        let summary = lfs_fetch(git, &path, &self.locks, opts).await?;
        info!(
            pointers = summary.pointers,
            fetched = summary.fetched,
            "lfs_fetch ok"
        );
        Ok(())
    }

    /// Enqueue one follow-up job (LFS, submodule, …). Best-effort by
    /// design: an already-queued job (same repo + type) is a no-op
    /// via the dedupe key, and any other failure is logged and
    /// swallowed — it must never fail the job that just succeeded.
    async fn enqueue_followup_job(
        &self,
        ctx: &JobContext,
        repo: &Repository,
        job_type: JobType,
        label: &str,
    ) {
        match create_job(
            &ctx.pool,
            NewJob {
                repository_id: repo.id,
                job_type,
                priority: ctx.job.priority,
            },
        )
        .await
        {
            Ok(job) => info!(job_id = %job.id, repo = %repo.full_name, "{label} step enqueued"),
            Err(e) if is_unique_violation(&e) => {
                debug!(repo = %repo.full_name, "{label} step already queued");
            }
            Err(e) => {
                tracing::warn!(repo = %repo.full_name, error = %e, "{label} step enqueue failed");
            }
        }
    }

    /// Enqueue a separate `LfsFetch` job when the repository has LFS
    /// pointers. Best-effort by design: git-lfs missing, no pointers,
    /// or an already-queued LFS job all mean "no step", not an error.
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
        self.enqueue_followup_job(ctx, repo, JobType::LfsFetch, "LFS")
            .await;
    }

    /// Enqueue a separate `SubmoduleSync` job when either local
    /// clone has submodules at HEAD. Best-effort, same as the LFS
    /// step.
    async fn maybe_enqueue_submodule_step(
        &self,
        ctx: &JobContext,
        repo: &Repository,
        layout: &RepositoryLayout,
        git: &Path,
    ) {
        let mirror_has =
            layout.mirror_path.is_dir() && has_submodules(git, &layout.mirror_path).await;
        let working_has = !mirror_has
            && layout.working_path.is_dir()
            && has_submodules(git, &layout.working_path).await;
        if !mirror_has && !working_has {
            return;
        }
        self.enqueue_followup_job(ctx, repo, JobType::SubmoduleSync, "submodule")
            .await;
    }

    async fn run_submodule_sync(
        &self,
        _ctx: &JobContext,
        git: &Path,
        repo: &Repository,
        layout: &RepositoryLayout,
        opts: SyncOptions,
    ) -> Result<(), RepoNyxError> {
        let mirror_exists = layout.mirror_path.is_dir();
        let working_exists = layout.working_path.is_dir();
        if !mirror_exists && !working_exists {
            return Err(RepoNyxError::new(
                ErrorCode::RepositoryNotFound,
                "no local clone for submodule sync; run a clone job first",
            )
            .with_details(format!("repo={}", repo.full_name)));
        }
        let mut total = 0;
        let mut synced = 0;
        let mut failures: Vec<SubmoduleFailure> = Vec::new();
        if mirror_exists {
            let summary =
                sync_submodules_mirror(git, &layout.mirror_path, &self.locks, opts.clone()).await?;
            total += summary.total;
            synced += summary.synced;
            failures.extend(summary.failed);
        }
        if working_exists {
            let summary =
                sync_submodules_working(git, &layout.working_path, &self.locks, opts).await?;
            total += summary.total;
            synced += summary.synced;
            failures.extend(summary.failed);
        }
        if total == 0 {
            info!(repo = %repo.full_name, "submodule sync: no submodules at HEAD");
            return Ok(());
        }
        info!(
            repo = %repo.full_name,
            total,
            synced,
            failed = failures.len(),
            "submodule sync done"
        );
        if synced == 0 {
            // Nothing at all synced: fail with the first real error
            // so the job is visible (and retried per policy — auth
            // failures are permanent and fail fast).
            let first = failures.into_iter().next().unwrap();
            return Err(first.error);
        }
        for f in &failures {
            tracing::warn!(repo = %repo.full_name, path = %f.path, error = %f.error, "submodule failed; others synced");
        }
        Ok(())
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
            // Coarse stage markers carry no number; the UI shows an
            // indeterminate bar until git reports a percentage.
            percent: None,
        });
    }
}
