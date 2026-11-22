//! Submodule sync (Phase 9).
//!
//! A superproject records submodules two ways: gitlink entries
//! (mode 160000) in its trees point at submodule commits, and the
//! `.gitmodules` blob at HEAD maps paths to URLs. The submodule
//! CONTENT lives in other repositories, so a mirror of the
//! superproject alone does not archive it. This module closes that
//! gap two ways:
//!
//! - Mirror: fetch each gitlink commit into the mirror's object
//!   database (`git fetch <url> <sha>`, verified with
//!   `cat-file -e`). The mirror then holds every object the
//!   superproject references, even though it has no worktree.
//! - Working clone: `git submodule update --init --recursive` per
//!   submodule, so one inaccessible (e.g. private) submodule never
//!   blocks the others.
//!
//! Failures are per-submodule and reported, never hidden: the
//! summary lists every submodule that could not be synced, and the
//! caller (the queue runner) fails the job only when nothing at
//! all could be synced. A repo with no submodules is a no-op.

use crate::commands::{
    build_cat_file_exists, build_fetch_sha, build_ls_tree, build_remote_get_url,
    build_show_gitmodules, build_submodule_status, build_submodule_update,
};
use crate::locks::RepoLockManager;
use crate::options::SyncOptions;
use crate::process::{map_git_failure, run_git_command, run_git_command_streaming};
use crate::refs::{REF_TIMEOUT, git_dir_of};
use reponyx_core::{ErrorCode, RepoNyxError};
use std::collections::BTreeMap;
use std::path::Path;

/// One submodule declared at HEAD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmoduleInfo {
    /// Name from `.gitmodules` (`[submodule "<name>"]`); the path
    /// when no declaration exists.
    pub name: String,
    /// Path in the superproject.
    pub path: String,
    /// Clone URL from `.gitmodules`; `None` when undeclared.
    pub url: Option<String>,
    /// Commit the superproject's HEAD points at (gitlink sha).
    pub sha: Option<String>,
}

/// One submodule that could not be synced, with the real error.
#[derive(Debug, Clone)]
pub struct SubmoduleFailure {
    pub path: String,
    pub error: RepoNyxError,
}

/// Outcome of a submodule sync pass.
#[derive(Debug, Clone, Default)]
pub struct SubmoduleSyncSummary {
    /// Submodules found at HEAD.
    pub total: usize,
    /// Submodules whose content is now present.
    pub synced: usize,
    /// Submodules that failed, each with its own error.
    pub failed: Vec<SubmoduleFailure>,
}

impl SubmoduleSyncSummary {
    /// True when nothing failed (including the no-submodule case).
    pub fn is_ok(&self) -> bool {
        self.failed.is_empty()
    }
}

/// Parse `git ls-tree -r HEAD` output: gitlinks are lines
/// `160000 commit <sha>\t<path>`. Returns `(path, sha)` pairs.
pub fn parse_ls_tree_gitlinks(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let (meta, path) = line.split_once('\t')?;
            let mut parts = meta.split_whitespace();
            if parts.next()? != "160000" || parts.next()? != "commit" {
                return None;
            }
            let sha = parts.next()?.to_string();
            Some((path.to_string(), sha))
        })
        .collect()
}

/// Parse a `.gitmodules` blob (ini text) into
/// name -> (path, url). Sections are `[submodule "<name>"]`;
/// `path`/`url` keys are `key = value` lines.
pub fn parse_gitmodules(output: &str) -> BTreeMap<String, (Option<String>, Option<String>)> {
    let mut map: BTreeMap<String, (Option<String>, Option<String>)> = BTreeMap::new();
    let mut current: Option<String> = None;
    for raw in output.lines() {
        let line = raw.trim();
        if let Some(name) = line
            .strip_prefix("[submodule \"")
            .and_then(|rest| rest.strip_suffix("\"]"))
        {
            current = Some(name.to_string());
            map.entry(name.to_string()).or_default();
            continue;
        }
        if line.starts_with('[') {
            current = None;
            continue;
        }
        let Some(name) = &current else { continue };
        if let Some((key, value)) = line.split_once('=') {
            let entry = map.entry(name.clone()).or_default();
            match key.trim() {
                "path" => entry.0 = Some(value.trim().to_string()),
                "url" => entry.1 = Some(value.trim().to_string()),
                _ => {}
            }
        }
    }
    map
}

/// Parse `git submodule status --recursive` output: lines are
/// `<marker><sha> <path> ...` where the marker is ` `, `-`, `+`
/// or `U`. Returns `(path, sha)` pairs.
pub fn parse_submodule_status(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.strip_prefix([' ', '-', '+', 'U'])?;
            let mut parts = line.split_whitespace();
            let sha = parts.next()?.to_string();
            let path = parts.next()?.to_string();
            if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                Some((path, sha))
            } else {
                None
            }
        })
        .collect()
}

/// List the submodules declared at HEAD (gitlinks + `.gitmodules`),
/// for a bare mirror or a working clone. An empty repository (no
/// HEAD yet) has no submodules; a missing `.gitmodules` blob means
/// the gitlinks carry no URLs.
pub async fn list_submodules(
    git: &Path,
    repo_path: &Path,
) -> Result<Vec<SubmoduleInfo>, RepoNyxError> {
    let git_dir = git_dir_of(repo_path).ok_or_else(|| {
        RepoNyxError::new(ErrorCode::RepositoryNotFound, "not a git repository")
            .with_details(format!("path={}", repo_path.display()))
    })?;
    let cmd = build_ls_tree(git, &git_dir);
    let out = run_git_command(&cmd, REF_TIMEOUT, crate::process::never_cancel()).await?;
    if !out.success {
        // No HEAD yet (empty repo): no submodules to sync.
        if out.stderr.contains("Not a valid object name HEAD") {
            return Ok(Vec::new());
        }
        return Err(map_git_failure(&cmd, &out));
    }
    let gitlinks = parse_ls_tree_gitlinks(&out.stdout);
    if gitlinks.is_empty() {
        return Ok(Vec::new());
    }
    let decls = {
        let cmd = build_show_gitmodules(git, &git_dir);
        let out = run_git_command(&cmd, REF_TIMEOUT, crate::process::never_cancel()).await?;
        if out.success {
            parse_gitmodules(&out.stdout)
        } else {
            BTreeMap::new()
        }
    };
    // Index declarations by path for the join.
    let by_path: BTreeMap<String, (String, Option<String>)> = decls
        .into_iter()
        .filter_map(|(name, (path, url))| path.map(|p| (p, (name, url))))
        .collect();
    Ok(gitlinks
        .into_iter()
        .map(|(path, sha)| {
            let (name, url) = by_path
                .get(&path)
                .map(|(n, u)| (n.clone(), u.clone()))
                .unwrap_or_else(|| (path.clone(), None));
            SubmoduleInfo {
                name,
                path,
                url,
                sha: Some(sha),
            }
        })
        .collect())
}

/// True when the repository has any submodules at HEAD. A list
/// error means "cannot tell" and is treated as no submodules —
/// the caller logs it; the submodule step is best-effort.
pub async fn has_submodules(git: &Path, repo_path: &Path) -> bool {
    match list_submodules(git, repo_path).await {
        Ok(list) => !list.is_empty(),
        Err(e) => {
            tracing::debug!(error = %e, "submodule check failed; skipping submodule step");
            false
        }
    }
}

async fn object_exists(git: &Path, git_dir: &Path, sha: &str) -> Result<bool, RepoNyxError> {
    let cmd = build_cat_file_exists(git, git_dir, sha);
    let out = run_git_command(&cmd, REF_TIMEOUT, crate::process::never_cancel()).await?;
    Ok(out.success)
}

/// Resolve a submodule URL against the superproject's origin URL,
/// the way git resolves relative submodule URLs (`../other.git`,
/// `./sibling.git`). Absolute URLs (anything with a scheme,
/// `user@host:path`, or a leading `/`) are returned unchanged.
/// A relative URL with no origin to resolve against is returned
/// unchanged — the fetch will fail honestly with its own error.
pub fn resolve_submodule_url(origin_url: Option<&str>, url: &str) -> String {
    if url.starts_with("./") || url.starts_with("../") {
        let Some(origin) = origin_url else {
            return url.to_string();
        };
        // Split the origin into (prefix, path): the prefix is the
        // scheme+authority (`https://github.com`), the SCP head
        // (`git@github.com:`), or empty for a plain path.
        let (prefix, path) = if let Some(idx) = origin.find("://") {
            let (scheme, rest) = origin.split_at(idx + 3);
            match rest.find('/') {
                Some(slash) => {
                    let (authority, path) = rest.split_at(slash);
                    (format!("{scheme}{authority}"), path.to_string())
                }
                None => (origin.to_string(), "/".to_string()),
            }
        } else if let Some(idx) = origin.find(':')
            && origin[..idx]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '@' || c == '.')
            && !origin[..idx].contains('/')
        {
            let (head, path) = origin.split_at(idx + 1);
            (head.to_string(), path.to_string())
        } else {
            (String::new(), origin.to_string())
        };
        // The relative URL resolves against the FULL origin URL
        // treated as a directory (git's own rule: `../x` consumes
        // the superproject's last segment, `./x` does not).
        let joined = format!("{path}/{url}");
        // Normalize `.`/`..` segments lexically (never touches the
        // prefix, so the scheme/host are safe), then re-attach the
        // prefix: an SCP prefix already ends in `:` (no slash),
        // a scheme prefix or an empty prefix needs one.
        let mut out: Vec<&str> = Vec::new();
        for seg in joined.split('/') {
            match seg {
                "" | "." => {}
                ".." => {
                    out.pop();
                }
                s => out.push(s),
            }
        }
        let norm = out.join("/");
        if prefix.ends_with(':') {
            format!("{prefix}{norm}")
        } else {
            format!("{prefix}/{norm}")
        }
    } else {
        url.to_string()
    }
}

async fn origin_url(git: &Path, git_dir: &Path) -> Option<String> {
    let cmd = build_remote_get_url(git, git_dir);
    let out = run_git_command(&cmd, REF_TIMEOUT, crate::process::never_cancel())
        .await
        .ok()?;
    if out.success {
        Some(out.stdout.trim().to_string())
    } else {
        None
    }
}

/// Fetch each submodule commit into a mirror's object database.
/// One failure never blocks the others; every failure is reported
/// in the summary. The per-repository lock is held for the whole
/// pass (same lock as mirror clone/fetch).
pub async fn sync_submodules_mirror(
    git: &Path,
    mirror_path: &Path,
    locks: &RepoLockManager,
    opts: SyncOptions,
) -> Result<SubmoduleSyncSummary, RepoNyxError> {
    let git_dir = git_dir_of(mirror_path).ok_or_else(|| {
        RepoNyxError::new(
            ErrorCode::RepositoryNotFound,
            "mirror path is not a git repository",
        )
        .with_details(format!("path={}", mirror_path.display()))
    })?;
    let subs = list_submodules(git, mirror_path).await?;
    let mut summary = SubmoduleSyncSummary {
        total: subs.len(),
        ..Default::default()
    };
    if subs.is_empty() {
        return Ok(summary);
    }
    let origin = origin_url(git, &git_dir).await;
    let _lock = locks.acquire(mirror_path, opts.lock_timeout).await?;
    for sub in subs {
        let (Some(url), Some(sha)) = (sub.url.clone(), sub.sha.clone()) else {
            summary.failed.push(SubmoduleFailure {
                path: sub.path.clone(),
                error: RepoNyxError::new(
                    ErrorCode::InvalidInput,
                    "submodule has no URL in .gitmodules or no gitlink sha; cannot fetch its objects",
                ),
            });
            continue;
        };
        if object_exists(git, &git_dir, &sha).await? {
            summary.synced += 1;
            continue;
        }
        let url = resolve_submodule_url(origin.as_deref(), &url);
        let mut cmd = build_fetch_sha(git, &git_dir, &url, &sha);
        if let Some(helper) = &opts.credential {
            helper.install(&mut cmd);
        }
        let out = run_git_command_streaming(
            &cmd,
            opts.timeout,
            opts.cancel.as_ref(),
            opts.progress.clone(),
        )
        .await;
        let fetch_err = match out {
            Ok(o) if o.success => None,
            Ok(o) => Some(map_git_failure(&cmd, &o)),
            Err(e) => Some(e),
        };
        if let Some(e) = fetch_err {
            summary.failed.push(SubmoduleFailure {
                path: sub.path.clone(),
                error: e,
            });
            continue;
        }
        if object_exists(git, &git_dir, &sha).await? {
            summary.synced += 1;
        } else {
            summary.failed.push(SubmoduleFailure {
                path: sub.path.clone(),
                error: RepoNyxError::new(
                    ErrorCode::SubmoduleAccessDenied,
                    "submodule commit still missing after fetch",
                )
                .with_details(format!("sha={sha}")),
            });
        }
    }
    Ok(summary)
}

/// Check out each submodule of a working clone (`update --init
/// --recursive`, one submodule at a time so failures stay
/// isolated). The per-repository lock is held for the whole pass.
pub async fn sync_submodules_working(
    git: &Path,
    work_path: &Path,
    locks: &RepoLockManager,
    opts: SyncOptions,
) -> Result<SubmoduleSyncSummary, RepoNyxError> {
    if git_dir_of(work_path).is_none() {
        return Err(RepoNyxError::new(
            ErrorCode::RepositoryNotFound,
            "working path is not a git repository",
        )
        .with_details(format!("path={}", work_path.display())));
    }
    let cmd = build_submodule_status(git, work_path);
    let out = run_git_command(&cmd, REF_TIMEOUT, crate::process::never_cancel()).await?;
    if !out.success {
        return Err(map_git_failure(&cmd, &out));
    }
    let subs = parse_submodule_status(&out.stdout);
    let mut summary = SubmoduleSyncSummary {
        total: subs.len(),
        ..Default::default()
    };
    if subs.is_empty() {
        return Ok(summary);
    }
    let _lock = locks.acquire(work_path, opts.lock_timeout).await?;
    for (path, _sha) in subs {
        let mut cmd = build_submodule_update(git, work_path, &path);
        if let Some(helper) = &opts.credential {
            helper.install(&mut cmd);
        }
        let out = run_git_command_streaming(
            &cmd,
            opts.timeout,
            opts.cancel.as_ref(),
            opts.progress.clone(),
        )
        .await;
        let update_err = match out {
            Ok(o) if o.success => None,
            Ok(o) => Some(map_git_failure(&cmd, &o)),
            Err(e) => Some(e),
        };
        match update_err {
            None => summary.synced += 1,
            Some(e) => summary.failed.push(SubmoduleFailure { path, error: e }),
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ls_tree_parses_gitlinks_only() {
        let out = "100644 blob ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34\tREADME.md\n\
                   160000 commit 957161dce6c65864066e98f463feee573d8242998094e2f6f186dbf9dbaa968c\tvendor/lib\n\
                   040000 tree 0123456789abcdef0123456789abcdef01234567\tsrc\n";
        let links = parse_ls_tree_gitlinks(out);
        assert_eq!(
            links,
            [(
                "vendor/lib".to_string(),
                "957161dce6c65864066e98f463feee573d8242998094e2f6f186dbf9dbaa968c".to_string()
            )]
        );
    }

    #[test]
    fn gitmodules_parses_sections() {
        let out = "[submodule \"vendor/lib\"]\n\tpath = vendor/lib\n\turl = https://github.com/o/lib.git\n[submodule \"docs\"]\n\tpath = docs\n\turl = ../docs.git\n";
        let map = parse_gitmodules(out);
        assert_eq!(
            map.get("vendor/lib"),
            Some(&(
                Some("vendor/lib".to_string()),
                Some("https://github.com/o/lib.git".to_string())
            ))
        );
        assert_eq!(
            map.get("docs"),
            Some(&(Some("docs".to_string()), Some("../docs.git".to_string())))
        );
    }

    #[test]
    fn submodule_status_parses_markers() {
        let out = " 957161dce6c65864066e98f463feee573d824299 vendor/lib (v1.0)\n\
                   -0123456789abcdef0123456789abcdef01234567 vendor/other (heads/main)\n";
        let subs = parse_submodule_status(out);
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].0, "vendor/lib");
        assert_eq!(subs[1].0, "vendor/other");
    }

    #[test]
    fn relative_submodule_url_resolves_against_origin() {
        assert_eq!(
            resolve_submodule_url(Some("/tmp/probe/super.git"), "../sub.git"),
            "/tmp/probe/sub.git"
        );
        assert_eq!(
            resolve_submodule_url(Some("https://github.com/o/super.git"), "../sub.git"),
            "https://github.com/o/sub.git"
        );
        assert_eq!(
            resolve_submodule_url(Some("git@github.com:o/super.git"), "../sub.git"),
            "git@github.com:o/sub.git"
        );
        assert_eq!(
            resolve_submodule_url(Some("/tmp/relprobe/a/b/super.git"), "./alsonosuch.git"),
            "/tmp/relprobe/a/b/super.git/alsonosuch.git"
        );
        assert_eq!(
            resolve_submodule_url(
                Some("https://github.com/o/super.git"),
                "https://github.com/other/lib.git"
            ),
            "https://github.com/other/lib.git"
        );
        assert_eq!(resolve_submodule_url(None, "../sub.git"), "../sub.git");
    }

    #[test]
    fn summary_ok_only_when_clean() {
        assert!(SubmoduleSyncSummary::default().is_ok());
        assert!(
            !SubmoduleSyncSummary {
                total: 1,
                synced: 0,
                failed: vec![SubmoduleFailure {
                    path: "x".into(),
                    error: RepoNyxError::new(ErrorCode::SubmoduleAccessDenied, "denied"),
                }],
            }
            .is_ok()
        );
    }
}
