//! On-disk storage usage: directory sizes and volume space.
//!
//! [`dir_size`] walks a directory tree and sums file sizes. It never
//! follows symlinks — a mirror must not double-count a linked tree
//! or loop forever — and it never fails the whole walk for one
//! unreadable entry: a missing path is simply 0 bytes. [`disk_usage`]
//! reports the volume holding a path, via `fs2`.
//!
//! Phase 9 adds storage management on top: [`scan_archive_dirs`]
//! enumerates the archive directories a storage root actually
//! holds (mirrors under `repositories/github/<account>/<owner>/`,
//! working clones under `working-clones/<account>/<owner>/`),
//! and [`delete_archive_dir`] removes one archive — but only
//! after proving the path is inside the root and really looks
//! like an archive. The caller (the desktop) decides which of the
//! scanned directories are orphans by matching them against the
//! repository rows; the engine never sees the database.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Total bytes of `path`: a file's length, or the recursive sum of
/// a directory's files. Missing paths are 0; unreadable entries are
/// skipped.
pub fn dir_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() {
        return 0;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries.flatten().map(|entry| dir_size(&entry.path())).sum()
}

/// Volume space for the filesystem holding `path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskUsage {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl DiskUsage {
    /// Bytes in use on the volume (`total - available`, saturating).
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }
}

/// Volume space for the filesystem holding `path`. Fails when the
/// path's filesystem cannot be queried (e.g. the path does not
/// exist yet).
pub fn disk_usage(path: &Path) -> std::io::Result<DiskUsage> {
    Ok(DiskUsage {
        total_bytes: fs2::total_space(path)?,
        available_bytes: fs2::available_space(path)?,
    })
}

/// Which side of an archive a directory holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    /// A bare mirror (`<name>.git`, HEAD at its top).
    Mirror,
    /// A working clone (a `.git` directory or file inside).
    Working,
}

impl ArchiveKind {
    /// `"mirror"` / `"working"` — the string the desktop shows.
    pub fn as_str(self) -> &'static str {
        match self {
            ArchiveKind::Mirror => "mirror",
            ArchiveKind::Working => "working",
        }
    }
}

/// One archive directory found on disk under a storage root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveDir {
    /// Absolute path of the archive directory.
    pub path: PathBuf,
    pub kind: ArchiveKind,
    /// Recursive size in bytes (see [`dir_size`]).
    pub bytes: u64,
}

/// Whether `path` looks like a mirror (a bare repo: HEAD + objects).
fn looks_like_mirror(path: &Path) -> bool {
    path.join("HEAD").is_file() && path.join("objects").is_dir()
}

/// Whether `path` looks like a working clone (a `.git` entry).
fn looks_like_working(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Enumerate the archive directories under a storage root:
/// mirrors at `<root>/repositories/github/<account>/<owner>/<name>.git`
/// and working clones at `<root>/working-clones/<account>/<owner>/<name>`.
/// A directory counts only when it looks like an archive (see
/// [`looks_like_mirror`] / [`looks_like_working`]) — stray folders
/// a user dropped into the tree are ignored. Missing trees are
/// simply empty; an unreadable level is skipped, never fatal.
pub fn scan_archive_dirs(root: &Path) -> Vec<ArchiveDir> {
    let mut found = Vec::new();
    let mirrors = root.join("repositories").join("github");
    for account in child_dirs(&mirrors) {
        for owner in child_dirs(&account) {
            for name in child_dirs(&owner) {
                if name
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(".git"))
                    && looks_like_mirror(&name)
                {
                    found.push(ArchiveDir {
                        bytes: dir_size(&name),
                        path: name,
                        kind: ArchiveKind::Mirror,
                    });
                }
            }
        }
    }
    let working = root.join("working-clones");
    for account in child_dirs(&working) {
        for owner in child_dirs(&account) {
            for name in child_dirs(&owner) {
                if looks_like_working(&name) {
                    found.push(ArchiveDir {
                        bytes: dir_size(&name),
                        path: name,
                        kind: ArchiveKind::Working,
                    });
                }
            }
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// Subdirectories of `path`, ignoring files, symlinks, and errors.
fn child_dirs(path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.is_dir() && !p.is_symlink())
        .collect()
}

/// Delete one archive directory, safely. The path must canonicalize
/// to somewhere inside the canonicalized root AND look like an
/// archive (mirror or working clone) — anything else is refused,
/// so a mistaken path can never delete arbitrary data. Returns the
/// bytes freed (measured before the delete).
pub fn delete_archive_dir(root: &Path, path: &Path) -> io::Result<u64> {
    let root = root
        .canonicalize()
        .map_err(|e| io::Error::new(e.kind(), format!("storage root is not readable: {e}")))?;
    let target = path.canonicalize().map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("archive path does not exist: {}", path.display()),
        )
    })?;
    if !target.starts_with(&root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "archive path {} is outside the storage root {}",
                target.display(),
                root.display()
            ),
        ));
    }
    if target == root {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to delete the storage root itself",
        ));
    }
    if !looks_like_mirror(&target) && !looks_like_working(&target) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "path {} does not look like a mirror or working clone",
                target.display()
            ),
        ));
    }
    let bytes = dir_size(&target);
    fs::remove_dir_all(&target)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(path: &Path, size: usize) {
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&vec![b'x'; size]).unwrap();
    }

    #[test]
    fn dir_size_counts_files_recursively() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.txt"), 10);
        write_file(&dir.path().join("b.txt"), 20);
        let nested = dir.path().join("sub");
        fs::create_dir(&nested).unwrap();
        write_file(&nested.join("c.txt"), 5);
        assert_eq!(dir_size(dir.path()), 35);
        assert_eq!(dir_size(&dir.path().join("a.txt")), 10);
    }

    #[test]
    fn dir_size_ignores_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(dir_size(&dir.path().join("nope")), 0);
        assert_eq!(dir_size(&dir.path().join("nope").join("deeper")), 0);
    }

    #[test]
    #[cfg(unix)]
    fn dir_size_skips_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        write_file(&real, 7);
        std::os::unix::fs::symlink(&real, dir.path().join("link.txt")).unwrap();
        // Only the real file counts — the link itself adds nothing.
        assert_eq!(dir_size(dir.path()), 7);
        // A symlink on its own is 0, never followed.
        assert_eq!(dir_size(&dir.path().join("link.txt")), 0);
    }

    #[test]
    fn disk_usage_reports_sane_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let usage = disk_usage(dir.path()).unwrap();
        assert!(usage.total_bytes > 0);
        assert!(usage.available_bytes <= usage.total_bytes);
        assert_eq!(
            usage.used_bytes(),
            usage.total_bytes - usage.available_bytes
        );
    }

    #[test]
    fn disk_usage_fails_for_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        assert!(disk_usage(&dir.path().join("nope")).is_err());
    }

    fn make_mirror(path: &Path) {
        fs::create_dir_all(path.join("objects")).unwrap();
        fs::create_dir_all(path.join("refs")).unwrap();
        write_file(&path.join("HEAD"), 10);
    }

    fn make_working(path: &Path) {
        fs::create_dir_all(path.join(".git")).unwrap();
        write_file(&path.join("README.md"), 5);
    }

    #[test]
    fn scan_finds_archives_and_skips_stray_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        make_mirror(
            &root
                .join("repositories")
                .join("github")
                .join("octocat")
                .join("octocat")
                .join("hello.git"),
        );
        // A .git dir without HEAD/objects is not an archive.
        fs::create_dir_all(
            root.join("repositories")
                .join("github")
                .join("octocat")
                .join("octocat")
                .join("fake.git"),
        )
        .unwrap();
        make_working(
            &root
                .join("working-clones")
                .join("octocat")
                .join("octocat")
                .join("hello"),
        );
        // A stray folder with no .git is ignored.
        fs::create_dir_all(
            root.join("working-clones")
                .join("octocat")
                .join("octocat")
                .join("notes"),
        )
        .unwrap();

        let found = scan_archive_dirs(root);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|a| a.kind == ArchiveKind::Mirror
            && a.path.ends_with("hello.git")
            && a.bytes == 10));
        assert!(
            found.iter().any(|a| a.kind == ArchiveKind::Working
                && a.path.ends_with("hello")
                && a.bytes == 5)
        );
    }

    #[test]
    fn scan_on_empty_root_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(scan_archive_dirs(dir.path()).is_empty());
    }

    #[test]
    fn delete_archive_dir_refuses_outside_and_non_archive() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir(&root).unwrap();
        let outside = dir.path().join("outside");
        fs::create_dir(&outside).unwrap();
        assert!(delete_archive_dir(&root, &outside).is_err());
        assert!(delete_archive_dir(&root, &root).is_err());
        let plain = root.join("plain");
        fs::create_dir(&plain).unwrap();
        write_file(&plain.join("x.txt"), 3);
        assert!(delete_archive_dir(&root, &plain).is_err());
        assert!(plain.exists(), "refused delete must not remove anything");
    }

    #[test]
    fn delete_archive_dir_removes_mirror_and_reports_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let mirror = root
            .join("repositories")
            .join("github")
            .join("octocat")
            .join("octocat")
            .join("hello.git");
        make_mirror(&mirror);
        let freed = delete_archive_dir(&root, &mirror).unwrap();
        assert_eq!(freed, 10);
        assert!(!mirror.exists());
        // Deleting again fails: the path is gone.
        assert!(delete_archive_dir(&root, &mirror).is_err());
    }
}
