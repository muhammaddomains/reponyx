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
}

impl GitCommand {
    pub fn new(program: impl Into<PathBuf>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            current_dir: None,
        }
    }

    pub fn with_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(dir.into());
        self
    }

    /// argv as it would appear to the process (program + args).
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
}
