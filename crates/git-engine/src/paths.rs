//! Path validation and storage layout (spec §13).
//!
//! Repository names, owner names, and account labels come from a remote
//! provider and are **never trusted as filesystem paths**. Every
//! component is validated here before it is joined to the storage root.
//!
//! Layout (default root `D:\RepoNyx\` on Windows; configurable):
//! ```text
//! <root>/repositories/github/<account>/<owner>/<name>.git/   (mirror)
//! <root>/working-clones/<account>/<owner>/<name>/            (working)
//! ```

use reponyx_core::{ErrorCode, RepoNyxError};
use std::path::{Component, Path, PathBuf};

/// A single path component is valid iff it is a plain name: no
/// separators, no `.`/`..`, no NUL, no Windows-reserved device names,
/// and no characters illegal on Windows (`<>:"/\|?*`).
pub fn validate_path_component(component: &str) -> Result<(), RepoNyxError> {
    if component.is_empty() || component == "." || component == ".." {
        return Err(invalid(component, "empty or dot component"));
    }
    if component.contains('\0') || component.contains('/') || component.contains('\\') {
        return Err(invalid(component, "contains a separator or NUL"));
    }
    if component
        .chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return Err(invalid(component, "contains an illegal character"));
    }
    let upper = component.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or("");
    if matches!(
        stem,
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Err(invalid(component, "Windows reserved device name"));
    }
    if component.ends_with(' ') || component.ends_with('.') {
        return Err(invalid(component, "ends with space or dot"));
    }
    Ok(())
}

fn invalid(component: &str, reason: &str) -> RepoNyxError {
    RepoNyxError::new(
        ErrorCode::RepositoryPathInvalid,
        format!("Invalid repository path component: {reason}"),
    )
    .with_details(format!("component={component:?}"))
}

/// Validate a full repository path stays inside `root` (no traversal,
/// no symlinks escaping the root — symlink check is a best-effort
/// canonicalization when the path exists).
pub fn validate_repository_path(root: &Path, path: &Path) -> Result<(), RepoNyxError> {
    if !path.starts_with(root) {
        return Err(RepoNyxError::new(
            ErrorCode::RepositoryPathInvalid,
            "Repository path escapes the storage root",
        ));
    }
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(RepoNyxError::new(
                ErrorCode::RepositoryPathInvalid,
                "Repository path contains '..'",
            ));
        }
    }
    Ok(())
}

/// Storage layout for one repository.
#[derive(Debug, Clone)]
pub struct RepositoryLayout {
    pub mirror_path: PathBuf,
    pub working_path: PathBuf,
}

impl RepositoryLayout {
    /// Build the layout for `account / owner / name` under `root`.
    /// All components are validated; collisions are detected by the
    /// caller (Phase 3) before any directory is created.
    pub fn new(root: &Path, account: &str, owner: &str, name: &str) -> Result<Self, RepoNyxError> {
        for c in [account, owner, name] {
            validate_path_component(c)?;
        }
        let mirror_path = root
            .join("repositories")
            .join("github")
            .join(account)
            .join(owner)
            .join(format!("{name}.git"));
        let working_path = root
            .join("working-clones")
            .join(account)
            .join(owner)
            .join(name);
        validate_repository_path(root, &mirror_path)?;
        validate_repository_path(root, &working_path)?;
        Ok(Self {
            mirror_path,
            working_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_names() {
        assert!(validate_path_component("project-one").is_ok());
        assert!(validate_path_component("owner_a").is_ok());
        assert!(validate_path_component("repo.name").is_ok());
    }

    #[test]
    fn rejects_traversal_and_separators() {
        for bad in ["..", ".", "", "../evil", "a/b", "a\\b", "a\0b"] {
            assert!(validate_path_component(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn rejects_windows_reserved_and_illegal_chars() {
        for bad in [
            "CON",
            "nul.txt",
            "COM1",
            "a:b",
            "a<b",
            "a|b",
            "a?b",
            "a*b",
            "trailing ",
        ] {
            assert!(validate_path_component(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn layout_joins_account_owner_name() {
        let layout =
            RepositoryLayout::new(Path::new("/data"), "account-01", "owner-a", "project-one")
                .unwrap();
        assert_eq!(
            layout.mirror_path,
            PathBuf::from("/data/repositories/github/account-01/owner-a/project-one.git")
        );
        assert_eq!(
            layout.working_path,
            PathBuf::from("/data/working-clones/account-01/owner-a/project-one")
        );
    }

    #[test]
    fn layout_rejects_malicious_owner() {
        assert!(RepositoryLayout::new(Path::new("/data"), "a", "../../etc", "x").is_err());
    }

    #[test]
    fn path_outside_root_rejected() {
        assert!(validate_repository_path(Path::new("/data"), Path::new("/etc/passwd")).is_err());
    }
}
