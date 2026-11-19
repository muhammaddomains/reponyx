//! Command builders: pure functions from inputs to argument arrays.
//!
//! Builders never execute anything and never embed credentials. Tests
//! assert the exact argv so a regression in a Git invocation is caught
//! before it reaches a real repository.

use std::path::{Path, PathBuf};

/// A fully-specified Git invocation: executable + argument array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Working directory, if the command needs one.
    pub current_dir: Option<PathBuf>,
    /// Extra environment variables for the child process. Secrets must
    /// never be passed here except via the credential-helper mechanism
    /// (`credentials`), which sets them explicitly and documents why.
    pub env: Vec<(String, String)>,
}

impl GitCommand {
    pub fn new(program: impl Into<PathBuf>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            current_dir: None,
            env: Vec::new(),
        }
    }

    pub fn with_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(dir.into());
        self
    }

    /// Add one environment variable for the child process.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// argv as it would appear to the process (program + args).
    /// Environment variables are never part of argv.
    pub fn argv(&self) -> Vec<String> {
        std::iter::once(self.program.to_string_lossy().into_owned())
            .chain(self.args.iter().cloned())
            .collect()
    }
}

/// `git clone --mirror <url> <dest>` — default archival mode (spec §9 Mode A).
/// The URL must be credential-free; credentials are injected separately.
pub fn build_clone_mirror(git: &Path, url: &str, destination: &Path) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            "clone".into(),
            "--mirror".into(),
            url.to_string(),
            destination.to_string_lossy().into_owned(),
        ],
    )
}

/// `git clone <url> <dest>` — working clone (Mode B).
pub fn build_clone_working(git: &Path, url: &str, destination: &Path) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            "clone".into(),
            url.to_string(),
            destination.to_string_lossy().into_owned(),
        ],
    )
}

/// `git --git-dir=<path> fetch --prune [--tags]` — mirror sync.
pub fn build_fetch(git: &Path, git_dir: &Path, fetch_tags: bool) -> GitCommand {
    let mut args = vec![
        format!("--git-dir={}", git_dir.to_string_lossy()),
        "fetch".into(),
        "--prune".into(),
    ];
    if fetch_tags {
        args.push("--tags".into());
    }
    GitCommand::new(git, args)
}

/// `git --git-dir=<path> fetch --all --prune --tags` — working-clone sync.
pub fn build_fetch_all(git: &Path, git_dir: &Path) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            format!("--git-dir={}", git_dir.to_string_lossy()),
            "fetch".into(),
            "--all".into(),
            "--prune".into(),
            "--tags".into(),
        ],
    )
}

/// `git ls-remote <url>` — probe reachability / refs without cloning.
pub fn build_ls_remote(git: &Path, url: &str) -> GitCommand {
    GitCommand::new(git, vec!["ls-remote".into(), url.to_string()])
}

/// `git clone --mirror --progress <url> <dest>` — mirror clone with
/// progress output forced on (stderr is not a TTY when piped).
pub fn build_clone_mirror_progress(git: &Path, url: &str, destination: &Path) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            "clone".into(),
            "--mirror".into(),
            "--progress".into(),
            url.to_string(),
            destination.to_string_lossy().into_owned(),
        ],
    )
}

/// `git --git-dir=<path> fetch --prune [--tags] --progress`.
pub fn build_fetch_progress(git: &Path, git_dir: &Path, fetch_tags: bool) -> GitCommand {
    let mut args = vec![
        format!("--git-dir={}", git_dir.to_string_lossy()),
        "fetch".into(),
        "--prune".into(),
    ];
    if fetch_tags {
        args.push("--tags".into());
    }
    args.push("--progress".into());
    GitCommand::new(git, args)
}

/// `git --git-dir=<path> fetch --all --prune --tags --progress`.
pub fn build_fetch_all_progress(git: &Path, git_dir: &Path) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            format!("--git-dir={}", git_dir.to_string_lossy()),
            "fetch".into(),
            "--all".into(),
            "--prune".into(),
            "--tags".into(),
            "--progress".into(),
        ],
    )
}

/// `git --git-dir=<path> for-each-ref --format=<format> <patterns...>`
/// — branch / tag / ref discovery.
pub fn build_for_each_ref(
    git: &Path,
    git_dir: &Path,
    format: &str,
    patterns: &[&str],
) -> GitCommand {
    let mut args = vec![
        format!("--git-dir={}", git_dir.to_string_lossy()),
        "for-each-ref".into(),
        format!("--format={format}"),
    ];
    args.extend(patterns.iter().map(|p| (*p).to_string()));
    GitCommand::new(git, args)
}

/// `git -C <dir> rev-list --left-right --count <range>` — divergence
/// (ahead/behind) between two refs, e.g. `HEAD...@{upstream}`.
pub fn build_rev_list_count(git: &Path, dir: &Path, range: &str) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            "-C".into(),
            dir.to_string_lossy().into_owned(),
            "rev-list".into(),
            "--left-right".into(),
            "--count".into(),
            range.to_string(),
        ],
    )
}

/// `git --git-dir=<path> fsck --no-dangling` — integrity check.
pub fn build_fsck(git: &Path, git_dir: &Path) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            format!("--git-dir={}", git_dir.to_string_lossy()),
            "fsck".into(),
            "--no-dangling".into(),
        ],
    )
}

/// `git --git-dir=<path> rev-parse --is-bare-repository`.
pub fn build_is_bare(git: &Path, git_dir: &Path) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            format!("--git-dir={}", git_dir.to_string_lossy()),
            "rev-parse".into(),
            "--is-bare-repository".into(),
        ],
    )
}

/// `git -C <dir> rev-parse --symbolic-full-name <ref>@{upstream}`
/// — the upstream of a branch (or HEAD), as a full ref such as
/// `refs/remotes/origin/main`.
pub fn build_rev_parse_upstream(git: &Path, dir: &Path, reference: &str) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            "-C".into(),
            dir.to_string_lossy().into_owned(),
            "rev-parse".into(),
            "--symbolic-full-name".into(),
            format!("{reference}@{{upstream}}"),
        ],
    )
}

/// `git --git-dir=<path> rev-parse --abbrev-ref HEAD` etc.
pub fn build_rev_parse(git: &Path, git_dir: &Path, rev: &str) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            format!("--git-dir={}", git_dir.to_string_lossy()),
            "rev-parse".into(),
            rev.to_string(),
        ],
    )
}

/// `git -C <dir> status --porcelain` — detect uncommitted changes.
pub fn build_status_porcelain(git: &Path, work_dir: &Path) -> GitCommand {
    GitCommand::new(
        git,
        vec![
            "-C".into(),
            work_dir.to_string_lossy().into_owned(),
            "status".into(),
            "--porcelain".into(),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_path() -> PathBuf {
        PathBuf::from("git")
    }

    #[test]
    fn mirror_clone_argv() {
        let cmd = build_clone_mirror(
            &git_path(),
            "https://github.com/owner/repo.git",
            Path::new("/data/owner/repo.git"),
        );
        assert_eq!(
            cmd.argv(),
            [
                "git",
                "clone",
                "--mirror",
                "https://github.com/owner/repo.git",
                "/data/owner/repo.git"
            ]
        );
    }

    #[test]
    fn fetch_prunes_and_optionally_fetches_tags() {
        let cmd = build_fetch(&git_path(), Path::new("/data/repo.git"), true);
        assert_eq!(
            cmd.argv(),
            [
                "git",
                "--git-dir=/data/repo.git",
                "fetch",
                "--prune",
                "--tags"
            ]
        );
        let cmd = build_fetch(&git_path(), Path::new("/data/repo.git"), false);
        assert!(!cmd.argv().contains(&"--tags".to_string()));
    }

    #[test]
    fn fetch_all_includes_all_and_tags() {
        let cmd = build_fetch_all(&git_path(), Path::new("/data/repo.git"));
        assert_eq!(
            cmd.argv(),
            [
                "git",
                "--git-dir=/data/repo.git",
                "fetch",
                "--all",
                "--prune",
                "--tags"
            ]
        );
    }

    #[test]
    fn ls_remote_and_status_argv() {
        assert_eq!(
            build_ls_remote(&git_path(), "https://github.com/o/r.git").argv(),
            ["git", "ls-remote", "https://github.com/o/r.git"]
        );
        assert_eq!(
            build_status_porcelain(&git_path(), Path::new("/work/repo")).argv(),
            ["git", "-C", "/work/repo", "status", "--porcelain"]
        );
    }

    #[test]
    fn urls_are_passed_as_single_arguments() {
        // A URL containing spaces/shell metacharacters must stay one argv
        // element — proof no shell splitting happens at this layer.
        let cmd = build_clone_mirror(
            &git_path(),
            "https://github.com/owner/repo name.git",
            Path::new("/data/x.git"),
        );
        assert_eq!(cmd.args[2], "https://github.com/owner/repo name.git");
    }

    #[test]
    fn progress_builders_force_progress_output() {
        assert_eq!(
            build_clone_mirror_progress(
                &git_path(),
                "https://github.com/o/r.git",
                Path::new("/data/r.git")
            )
            .argv(),
            [
                "git",
                "clone",
                "--mirror",
                "--progress",
                "https://github.com/o/r.git",
                "/data/r.git"
            ]
        );
        assert_eq!(
            build_fetch_progress(&git_path(), Path::new("/data/r.git"), true).argv(),
            [
                "git",
                "--git-dir=/data/r.git",
                "fetch",
                "--prune",
                "--tags",
                "--progress"
            ]
        );
        assert_eq!(
            build_fetch_all_progress(&git_path(), Path::new("/data/r.git")).argv(),
            [
                "git",
                "--git-dir=/data/r.git",
                "fetch",
                "--all",
                "--prune",
                "--tags",
                "--progress"
            ]
        );
    }

    #[test]
    fn discovery_and_health_builders() {
        assert_eq!(
            build_for_each_ref(
                &git_path(),
                Path::new("/data/r.git"),
                "%(refname)%00%(objectname)",
                &["refs/heads/", "refs/remotes/"]
            )
            .argv(),
            [
                "git",
                "--git-dir=/data/r.git",
                "for-each-ref",
                "--format=%(refname)%00%(objectname)",
                "refs/heads/",
                "refs/remotes/"
            ]
        );
        assert_eq!(
            build_rev_list_count(&git_path(), Path::new("/work/r"), "HEAD...@{upstream}").argv(),
            [
                "git",
                "-C",
                "/work/r",
                "rev-list",
                "--left-right",
                "--count",
                "HEAD...@{upstream}"
            ]
        );
        assert_eq!(
            build_fsck(&git_path(), Path::new("/data/r.git")).argv(),
            ["git", "--git-dir=/data/r.git", "fsck", "--no-dangling"]
        );
        assert_eq!(
            build_is_bare(&git_path(), Path::new("/data/r.git")).argv(),
            [
                "git",
                "--git-dir=/data/r.git",
                "rev-parse",
                "--is-bare-repository"
            ]
        );
        assert_eq!(
            build_rev_parse_upstream(&git_path(), Path::new("/work/r"), "HEAD").argv(),
            [
                "git",
                "-C",
                "/work/r",
                "rev-parse",
                "--symbolic-full-name",
                "HEAD@{upstream}"
            ]
        );
    }

    #[test]
    fn env_is_not_part_of_argv() {
        let cmd = GitCommand::new("git", vec!["--version".into()])
            .with_env("REPONYX_GIT_CREDENTIAL_PASSWORD", "s3cr3t");
        assert_eq!(cmd.argv(), ["git", "--version"]);
        assert_eq!(
            cmd.env,
            [(
                "REPONYX_GIT_CREDENTIAL_PASSWORD".to_string(),
                "s3cr3t".to_string()
            )]
        );
    }
}
