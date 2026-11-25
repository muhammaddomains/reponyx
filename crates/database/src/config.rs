//! Import/export of settings + selections (Phase 9).
//!
//! The config file is a versioned JSON document (`reponyx-config`,
//! version 1) holding the four non-secret settings, each
//! repository's sync selection (mode + enabled), and the schedules.
//! It never holds credentials, account tokens, local paths, or
//! archive state — those are machine-local and are not exported.
//!
//! Identity across machines uses provider-stable ids, never local
//! UUIDs: repositories match by `provider_repository_id`, accounts
//! by (`provider`, `provider_account_id`). An entry whose
//! repository or account does not exist on this machine is not an
//! error — it is skipped and reported, so the user can discover
//! that repository or connect that account first and import again.
//!
//! Import is atomic in the sense that matters: everything is
//! resolved and validated (settings values, schedule cadences and
//! timezones via the `next_run` callback) before anything is
//! written, so a malformed file refuses the whole import instead
//! of applying half of it. The `next_run` computation lives in
//! `reponyx-scheduler`, which this crate cannot depend on (it
//! depends on us), so the caller passes it in — the desktop passes
//! the real `next_run`, tests pass a stub.

use crate::accounts::{get_account_by_provider, list_accounts};
use crate::common::invalid;
use crate::diagnostics::is_safe_for_diagnostics;
use crate::repositories::{RepositoryFilter, list_repositories, set_sync_enabled, set_sync_mode};
use crate::schedules::{
    NewSchedule, SchedulePatch, create_schedule, get_global_schedule, get_schedule_for_account,
    get_schedule_for_repository, list_schedules, update_schedule,
};
use crate::settings::{list_settings, set_setting};
use chrono::{DateTime, Utc};
use reponyx_core::{IntervalType, Provider, RepoNyxError, Schedule, ScheduleScope, SyncMode};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Component, Path};
use uuid::Uuid;

/// The config file format marker. A file with any other marker is
/// refused, never guessed at.
pub const CONFIG_FORMAT: &str = "reponyx-config";
/// The only config version this app reads. A newer version is
/// refused rather than partially understood.
pub const CONFIG_VERSION: u32 = 1;

// -- The file format --------------------------------------------------

/// One settings row in the file. Only the five non-secret keys
/// (see [`validate_setting_value`]) are ever written or accepted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSetting {
    pub key: String,
    pub value: String,
}

/// One repository's sync selection. `local_path`, archive status,
/// and last-synced state are machine-local and deliberately absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigRepository {
    pub provider: Provider,
    pub provider_repository_id: String,
    pub provider_account_id: String,
    /// Display only — matching uses the provider ids, never this.
    pub account_username: String,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub clone_url: String,
    pub sync_mode: SyncMode,
    pub sync_enabled: bool,
}

/// One schedule. The target is identified by stable ids:
/// nothing for `global`, (`provider`, `provider_account_id`) for
/// `account`, (`provider`, `provider_repository_id`) for
/// `repository`. `account_username` / `repository_full_name` are
/// display only. `next_run_at` is never stored — it is recomputed
/// from the cadence on import, the same way creating a schedule
/// computes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchedule {
    pub scope: ScheduleScope,
    pub provider: Option<Provider>,
    pub provider_account_id: Option<String>,
    pub account_username: Option<String>,
    pub provider_repository_id: Option<String>,
    pub repository_full_name: Option<String>,
    pub interval_type: IntervalType,
    pub interval_value: Option<String>,
    pub timezone: String,
    pub enabled: bool,
}

/// The whole config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub format: String,
    pub version: u32,
    pub app_version: String,
    pub exported_at: String,
    #[serde(default)]
    pub settings: Vec<ConfigSetting>,
    #[serde(default)]
    pub repositories: Vec<ConfigRepository>,
    #[serde(default)]
    pub schedules: Vec<ConfigSchedule>,
}

/// One entry the import skipped because its repository or account
/// does not exist on this machine yet. Not an error — the user can
/// discover or connect it and import again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedRef {
    /// `"repository"` or `"schedule"`.
    pub kind: String,
    /// `full_name` for a repository, the schedule's target for a
    /// schedule.
    pub label: String,
    pub reason: String,
}

/// What an import did, for the report shown after it runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReport {
    pub settings_applied: usize,
    pub repositories_applied: usize,
    pub schedules_created: usize,
    pub schedules_updated: usize,
    pub skipped: Vec<SkippedRef>,
}

// -- Settings validation (single source of truth) ---------------------

/// Validate one setting value the way the Settings page does.
/// Unknown keys are refused — only the five non-secret settings
/// travel in a config file. The desktop `validate_setting` in
/// `settings.rs` delegates here so the rules live in one place.
pub fn validate_setting_value(key: &str, value: &str) -> Result<(), RepoNyxError> {
    match key {
        "notifications.enabled" => match value {
            "true" | "false" => Ok(()),
            _ => Err(invalid(format!(
                "notifications.enabled must be true or false (got {value})"
            ))),
        },
        "sync.default_mode" => match value {
            "mirror" | "working" | "both" => Ok(()),
            _ => Err(invalid(format!(
                "sync.default_mode must be mirror, working, or both (got {value})"
            ))),
        },
        "sync.concurrency" => {
            let parsed: i64 = value
                .parse()
                .map_err(|_| invalid("sync.concurrency must be a whole number between 1 and 10"))?;
            if (1..=10).contains(&parsed) {
                Ok(())
            } else {
                Err(invalid("sync.concurrency must be between 1 and 10"))
            }
        }
        "storage.root" => {
            if std::path::Path::new(value).is_absolute() {
                Ok(())
            } else {
                Err(invalid("storage.root must be an absolute path"))
            }
        }
        "ui.theme" => match value {
            "system" | "light" | "dark" => Ok(()),
            _ => Err(invalid(format!(
                "ui.theme must be system, light, or dark (got {value})"
            ))),
        },
        _ => Err(invalid(format!("unknown setting: {key}"))),
    }
}

/// Validate a `storage.root` value against the disk, for the Settings
/// page (same-machine only). The portable check in
/// [`validate_setting_value`] stays absolute-path-only on purpose: a
/// config file is cross-machine, and its root may not exist on the
/// machine importing it.
///
/// The checks run on the canonical path (symlinks and `..` resolved),
/// but the value itself is stored verbatim — the canonical form on
/// Windows carries a `\\?\` prefix the Settings page should not show.
/// Rejects:
/// - a relative path;
/// - a path that does not exist, or is not a directory;
/// - a drive root (`D:\`, `/`) — archive containment would be
///   meaningless there;
/// - the data directory, or any path containing it / contained by it
///   — the archive tree must never hold the database, and the
///   database directory must never sit inside the archive tree;
/// - a directory that is not writable. Permissions bits alone lie on
///   Windows, so this is a probe: a file is created and removed.
///   (A read-only directory the process can still write to — for
///   example when running as root or Administrator — passes, because
///   the probe is the truth, not the bits.)
pub fn validate_storage_root(value: &str, data_dir: &Path) -> Result<(), RepoNyxError> {
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err(invalid("storage.root must be an absolute path"));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| invalid(format!("storage.root does not exist: {value}")))?;
    if !canonical.is_dir() {
        return Err(invalid(format!("storage.root is not a directory: {value}")));
    }
    if !canonical
        .components()
        .any(|c| matches!(c, Component::Normal(_)))
    {
        return Err(invalid(format!(
            "storage.root must not be a drive root: {value}"
        )));
    }
    let data_canonical = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    if canonical.starts_with(&data_canonical) || data_canonical.starts_with(&canonical) {
        return Err(invalid(
            "storage.root must not be the data directory, contain it, or sit inside it",
        ));
    }
    let probe = canonical.join(format!(".reponyx-write-test-{}", std::process::id()));
    if let Err(e) = std::fs::write(&probe, b"") {
        return Err(invalid(format!(
            "storage.root is not writable: {value} ({e})"
        )));
    }
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

// -- Export -----------------------------------------------------------

/// Build the config file from the live database: the non-secret
/// settings, every repository's sync selection, and every schedule
/// with its target resolved to stable ids.
pub async fn build_config_export(
    pool: &SqlitePool,
    app_version: &str,
) -> Result<ConfigFile, RepoNyxError> {
    let settings = list_settings(pool)
        .await?
        .into_iter()
        .filter(|s| is_safe_for_diagnostics(&s.key))
        .map(|s| ConfigSetting {
            key: s.key,
            value: s.value,
        })
        .collect();

    let accounts = list_accounts(pool).await?;
    let accounts_by_id: HashMap<Uuid, &reponyx_core::Account> =
        accounts.iter().map(|a| (a.id, a)).collect();

    let repos = list_repositories(pool, RepositoryFilter::default()).await?;
    let repos_by_id: HashMap<Uuid, &reponyx_core::Repository> =
        repos.iter().map(|r| (r.id, r)).collect();

    let repositories = repos
        .iter()
        .filter_map(|r| {
            let account = accounts_by_id.get(&r.account_id)?;
            Some(ConfigRepository {
                provider: account.provider,
                provider_repository_id: r.provider_repository_id.clone(),
                provider_account_id: account.provider_account_id.clone(),
                account_username: account.username.clone(),
                owner: r.owner.clone(),
                name: r.name.clone(),
                full_name: r.full_name.clone(),
                clone_url: r.clone_url.clone(),
                sync_mode: r.sync_mode,
                sync_enabled: r.sync_enabled,
            })
        })
        .collect();

    let schedules = list_schedules(pool)
        .await?
        .into_iter()
        .filter_map(|s| {
            let (provider, provider_account_id, account_username) = match s.scope {
                ScheduleScope::Global => (None, None, None),
                ScheduleScope::Account => {
                    let account = s.account_id.and_then(|id| accounts_by_id.get(&id))?;
                    (
                        Some(account.provider),
                        Some(account.provider_account_id.clone()),
                        Some(account.username.clone()),
                    )
                }
                ScheduleScope::Repository => {
                    let repo = s.repository_id.and_then(|id| repos_by_id.get(&id))?;
                    let account = accounts_by_id.get(&repo.account_id)?;
                    (
                        Some(account.provider),
                        Some(account.provider_account_id.clone()),
                        Some(account.username.clone()),
                    )
                }
            };
            let (provider_repository_id, repository_full_name) = match s.scope {
                ScheduleScope::Repository => {
                    let repo = s.repository_id.and_then(|id| repos_by_id.get(&id))?;
                    (
                        Some(repo.provider_repository_id.clone()),
                        Some(repo.full_name.clone()),
                    )
                }
                _ => (None, None),
            };
            Some(ConfigSchedule {
                scope: s.scope,
                provider,
                provider_account_id,
                account_username,
                provider_repository_id,
                repository_full_name,
                interval_type: s.interval_type,
                interval_value: s.interval_value,
                timezone: s.timezone,
                enabled: s.enabled,
            })
        })
        .collect();

    Ok(ConfigFile {
        format: CONFIG_FORMAT.to_string(),
        version: CONFIG_VERSION,
        app_version: app_version.to_string(),
        exported_at: Utc::now().to_rfc3339(),
        settings,
        repositories,
        schedules,
    })
}

// -- Parse + shape validation -----------------------------------------

/// Parse config file text and check the format marker, version,
/// settings values, and schedule shapes. Anything malformed refuses
/// the whole file — the caller applies nothing after an error here.
pub fn parse_config_file(text: &str) -> Result<ConfigFile, RepoNyxError> {
    let file: ConfigFile = serde_json::from_str(text)
        .map_err(|e| invalid(format!("not a valid RepoNyx config file: {e}")))?;
    if file.format != CONFIG_FORMAT {
        return Err(invalid(format!("unknown config format: {}", file.format)));
    }
    if file.version != CONFIG_VERSION {
        return Err(invalid(format!(
            "unsupported config version: {} (this app reads version {CONFIG_VERSION})",
            file.version
        )));
    }
    for setting in &file.settings {
        validate_setting_value(&setting.key, &setting.value)?;
    }
    for repo in &file.repositories {
        if repo.provider_repository_id.trim().is_empty() {
            return Err(invalid(format!(
                "repository {} has no provider id",
                repo.full_name
            )));
        }
        if repo.owner.trim().is_empty()
            || repo.name.trim().is_empty()
            || repo.full_name.trim().is_empty()
            || repo.clone_url.trim().is_empty()
        {
            return Err(invalid(format!(
                "repository {} is missing owner, name, or clone URL",
                repo.full_name
            )));
        }
    }
    for schedule in &file.schedules {
        validate_schedule_shape(schedule)?;
    }
    Ok(file)
}

/// A schedule entry must name a target matching its scope, and its
/// timezone must not be empty. Cadence validity is checked later,
/// while planning (through the `next_run` callback), still before
/// anything is written.
fn validate_schedule_shape(schedule: &ConfigSchedule) -> Result<(), RepoNyxError> {
    match schedule.scope {
        ScheduleScope::Global => {
            if schedule.provider.is_some()
                || schedule.provider_account_id.is_some()
                || schedule.provider_repository_id.is_some()
            {
                return Err(invalid(
                    "a global schedule must not name an account or repository",
                ));
            }
        }
        ScheduleScope::Account => {
            let has_account_id = schedule
                .provider_account_id
                .as_deref()
                .is_some_and(|id| !id.trim().is_empty());
            if schedule.provider.is_none() || !has_account_id {
                return Err(invalid(
                    "an account schedule needs a provider and a provider account id",
                ));
            }
        }
        ScheduleScope::Repository => {
            let has_repository_id = schedule
                .provider_repository_id
                .as_deref()
                .is_some_and(|id| !id.trim().is_empty());
            if schedule.provider.is_none() || !has_repository_id {
                return Err(invalid(
                    "a repository schedule needs a provider and a provider repository id",
                ));
            }
        }
    }
    if schedule.timezone.trim().is_empty() {
        return Err(invalid("a schedule needs a timezone"));
    }
    Ok(())
}

// -- Apply ------------------------------------------------------------

/// What applying one schedule will do, resolved in the planning
/// phase so the whole import can be validated before any write.
enum SchedulePlan {
    Create(NewSchedule),
    Update { id: Uuid, patch: SchedulePatch },
}

/// Apply a parsed config file: settings, repository selections,
/// and schedules. Everything is resolved and validated first
/// (including every schedule's cadence, through `next_run`); only
/// then is anything written. Entries whose repository or account
/// does not exist locally are skipped and reported, not errors.
pub async fn apply_config_import(
    pool: &SqlitePool,
    file: &ConfigFile,
    next_run: impl Fn(&Schedule, DateTime<Utc>) -> Result<Option<DateTime<Utc>>, RepoNyxError>,
) -> Result<ImportReport, RepoNyxError> {
    let now = Utc::now();
    let mut skipped: Vec<SkippedRef> = Vec::new();

    // -- Phase 1: resolve + validate. No writes yet. --
    let local_repos = list_repositories(pool, RepositoryFilter::default()).await?;
    let mut repo_plan: Vec<(Uuid, SyncMode, bool)> = Vec::new();
    for repo in &file.repositories {
        match local_repos
            .iter()
            .find(|r| r.provider_repository_id == repo.provider_repository_id)
        {
            Some(local) => repo_plan.push((local.id, repo.sync_mode, repo.sync_enabled)),
            None => skipped.push(SkippedRef {
                kind: "repository".to_string(),
                label: repo.full_name.clone(),
                reason: "not on this machine yet — discover it from the Repositories page first"
                    .to_string(),
            }),
        }
    }

    let mut schedule_plan: Vec<SchedulePlan> = Vec::new();
    for sched in &file.schedules {
        let (account_id, repository_id) = match sched.scope {
            ScheduleScope::Global => (None, None),
            ScheduleScope::Account => {
                let provider = sched
                    .provider
                    .ok_or_else(|| invalid("an account schedule needs a provider"))?;
                let provider_account_id = sched
                    .provider_account_id
                    .as_deref()
                    .ok_or_else(|| invalid("an account schedule needs a provider account id"))?;
                match get_account_by_provider(pool, provider, provider_account_id).await? {
                    Some(account) => (Some(account.id), None),
                    None => {
                        skipped.push(SkippedRef {
                            kind: "schedule".to_string(),
                            label: sched
                                .account_username
                                .clone()
                                .unwrap_or_else(|| provider_account_id.to_string()),
                            reason: "that account is not connected on this machine yet".to_string(),
                        });
                        continue;
                    }
                }
            }
            ScheduleScope::Repository => {
                let provider_repository_id =
                    sched.provider_repository_id.as_deref().ok_or_else(|| {
                        invalid("a repository schedule needs a provider repository id")
                    })?;
                match local_repos
                    .iter()
                    .find(|r| r.provider_repository_id == provider_repository_id)
                {
                    Some(repo) => (None, Some(repo.id)),
                    None => {
                        skipped.push(SkippedRef {
                            kind: "schedule".to_string(),
                            label: sched
                                .repository_full_name
                                .clone()
                                .unwrap_or_else(|| provider_repository_id.to_string()),
                            reason: "that repository is not on this machine yet — discover it from the Repositories page first"
                                .to_string(),
                        });
                        continue;
                    }
                }
            }
        };

        let existing = match sched.scope {
            ScheduleScope::Global => get_global_schedule(pool).await?,
            ScheduleScope::Account => {
                get_schedule_for_account(pool, account_id.expect("account scope resolved")).await?
            }
            ScheduleScope::Repository => {
                get_schedule_for_repository(pool, repository_id.expect("repository scope resolved"))
                    .await?
            }
        };
        match existing {
            None => {
                let draft = Schedule {
                    id: Uuid::new_v4(),
                    scope: sched.scope,
                    account_id,
                    repository_id,
                    interval_type: sched.interval_type,
                    interval_value: sched.interval_value.clone(),
                    timezone: sched.timezone.clone(),
                    enabled: sched.enabled,
                    next_run_at: None,
                    created_at: now,
                    updated_at: now,
                };
                let next = next_run(&draft, now)?;
                schedule_plan.push(SchedulePlan::Create(NewSchedule {
                    scope: sched.scope,
                    account_id,
                    repository_id,
                    interval_type: sched.interval_type,
                    interval_value: sched.interval_value.clone(),
                    timezone: sched.timezone.clone(),
                    enabled: sched.enabled,
                    next_run_at: next,
                }));
            }
            Some(existing) => {
                let mut updated = existing.clone();
                let changed = updated.interval_type != sched.interval_type
                    || updated.interval_value != sched.interval_value
                    || updated.timezone != sched.timezone
                    || updated.enabled != sched.enabled;
                updated.interval_type = sched.interval_type;
                updated.interval_value = sched.interval_value.clone();
                updated.timezone = sched.timezone.clone();
                updated.enabled = sched.enabled;
                // Recompute the next run only when the cadence
                // changed and the schedule is enabled; the old
                // `next_run_at` stays the interval anchor, and a
                // disabled schedule keeps its stored value — the
                // same rule as updating a schedule from the UI.
                let next_run_at = if changed && updated.enabled {
                    Some(next_run(&updated, now)?)
                } else {
                    None
                };
                schedule_plan.push(SchedulePlan::Update {
                    id: existing.id,
                    patch: SchedulePatch {
                        interval_type: if changed {
                            Some(updated.interval_type)
                        } else {
                            None
                        },
                        interval_value: if changed {
                            Some(updated.interval_value.clone())
                        } else {
                            None
                        },
                        timezone: if changed {
                            Some(updated.timezone.clone())
                        } else {
                            None
                        },
                        enabled: if changed { Some(updated.enabled) } else { None },
                        next_run_at,
                    },
                });
            }
        }
    }

    // -- Phase 2: execute the plan. --
    let mut settings_applied = 0;
    for setting in &file.settings {
        set_setting(pool, &setting.key, &setting.value).await?;
        settings_applied += 1;
    }
    let mut repositories_applied = 0;
    for (id, mode, enabled) in repo_plan {
        set_sync_mode(pool, id, mode).await?;
        set_sync_enabled(pool, id, enabled).await?;
        repositories_applied += 1;
    }
    let mut schedules_created = 0;
    let mut schedules_updated = 0;
    for plan in schedule_plan {
        match plan {
            SchedulePlan::Create(input) => {
                create_schedule(pool, input).await?;
                schedules_created += 1;
            }
            SchedulePlan::Update { id, patch } => {
                update_schedule(pool, id, patch).await?;
                schedules_updated += 1;
            }
        }
    }

    Ok(ImportReport {
        settings_applied,
        repositories_applied,
        schedules_created,
        schedules_updated,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_enabled_accepts_booleans_only() {
        assert!(validate_setting_value("notifications.enabled", "true").is_ok());
        assert!(validate_setting_value("notifications.enabled", "false").is_ok());
        assert!(validate_setting_value("notifications.enabled", "yes").is_err());
        assert!(validate_setting_value("notifications.enabled", "1").is_err());
        assert!(validate_setting_value("notifications.enabled", "").is_err());
    }

    #[test]
    fn storage_root_validation_checks_the_disk() {
        let data = tempfile::tempdir().unwrap();
        let data_dir = data.path();
        let s = |p: &std::path::Path| p.to_str().unwrap().to_string();

        // Relative paths are refused.
        assert!(validate_storage_root("relative/path", data_dir).is_err());
        // A missing directory is refused.
        assert!(validate_storage_root(&s(&data_dir.join("nope")), data_dir).is_err());
        // A file is not a directory.
        let file = data_dir.join("file.txt");
        std::fs::write(&file, b"x").unwrap();
        assert!(validate_storage_root(&s(&file), data_dir).is_err());
        // A drive root is refused (Unix: `/`).
        assert!(validate_storage_root("/", data_dir).is_err());
        // The data directory itself is refused.
        assert!(validate_storage_root(&s(data_dir), data_dir).is_err());
        // A directory inside the data directory is refused.
        let inside = data_dir.join("sub");
        std::fs::create_dir(&inside).unwrap();
        assert!(validate_storage_root(&s(&inside), data_dir).is_err());
        // A directory containing the data directory is refused.
        assert!(validate_storage_root(&s(data_dir.parent().unwrap()), data_dir).is_err());
        // A `..` that resolves back to the data directory is refused.
        assert!(validate_storage_root(&s(&data_dir.join("sub").join("..")), data_dir).is_err());
        // A writable sibling directory passes, and the probe file is
        // removed again.
        let good = tempfile::tempdir().unwrap();
        assert!(validate_storage_root(&s(good.path()), data_dir).is_ok());
        assert_eq!(
            std::fs::read_dir(good.path()).unwrap().count(),
            0,
            "the writability probe must not leave a file behind"
        );
    }

    #[test]
    #[cfg(unix)]
    fn storage_root_follows_a_symlink_to_its_target() {
        use std::os::unix::fs::symlink;
        let data = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let link = data.path().join("link");
        symlink(target.path(), &link).unwrap();
        // The link points at a writable directory outside the data
        // dir, so it passes; the canonical check saw the target.
        assert!(validate_storage_root(link.to_str().unwrap(), data.path()).is_ok());
        // And a link to the data dir itself is refused.
        let bad_link = target.path().join("bad");
        symlink(data.path(), &bad_link).unwrap();
        assert!(validate_storage_root(bad_link.to_str().unwrap(), data.path()).is_err());
    }
}
