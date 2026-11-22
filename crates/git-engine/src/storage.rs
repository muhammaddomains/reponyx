//! On-disk storage usage: directory sizes and volume space.
//!
//! [`dir_size`] walks a directory tree and sums file sizes. It never
//! follows symlinks — a mirror must not double-count a linked tree
//! or loop forever — and it never fails the whole walk for one
//! unreadable entry: a missing path is simply 0 bytes. [`disk_usage`]
//! reports the volume holding a path, via `fs2`.

use std::fs;
use std::path::Path;

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
}
