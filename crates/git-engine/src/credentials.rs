//! Credential injection via a short-lived credential helper.
//!
//! Tokens are **never** placed in clone URLs, process arguments, or
//! logs. Instead, a temporary helper script is created for exactly
//! one operation (or a short series of operations) and passed to Git
//! via `-c credential.helper=<script>`.
//!
//! The script itself contains **no secret**: it prints the credential
//! from the `REPONYX_GIT_CREDENTIAL_USERNAME` /
//! `REPONYX_GIT_CREDENTIAL_PASSWORD` environment variables of the Git
//! child process. Environment variables do not appear in argv or in
//! process output, and the helper directory is deleted when the
//! [`CredentialHelper`] is dropped — including on failure or
//! cancellation, because dropping the guard is what deletes it.
//!
//! Rules enforced here:
//! - The helper script is created in a fresh temporary directory with
//!   `0700` permissions (script file `0700` on Unix) and removed on
//!   drop.
//! - `GIT_TERMINAL_PROMPT=0` is set on the Git command so Git can
//!   never fall back to an interactive password prompt (which would
//!   hang a non-interactive operation).
//! - The credential is rejected when it contains `\r` / `\n`, which
//!   would break the credential protocol and could smuggle extra
//!   protocol lines.
//! - URLs with embedded credentials (`https://user:pass@host/...`)
//!   are rejected before any Git invocation
//!   ([`reject_embedded_credentials`]).

use crate::commands::GitCommand;
use reponyx_core::{ErrorCode, RepoNyxError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Environment variables the helper script reads the credential from.
pub const CREDENTIAL_USERNAME_ENV: &str = "REPONYX_GIT_CREDENTIAL_USERNAME";
pub const CREDENTIAL_PASSWORD_ENV: &str = "REPONYX_GIT_CREDENTIAL_PASSWORD";

/// A username/password credential (in practice a GitHub username and a
/// fine-grained PAT, or a GitHub App installation token). The password
/// is never shown in `Debug` output.
pub struct Credential {
    pub username: String,
    password: String,
}

impl Credential {
    /// Create a credential. Rejects empty values and values containing
    /// CR/LF, which would break the `git credential` protocol.
    pub fn new(
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, RepoNyxError> {
        let username = username.into();
        let password = password.into();
        if username.is_empty() || password.is_empty() {
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "Credential username and password must not be empty",
            ));
        }
        if username.contains(['\r', '\n']) || password.contains(['\r', '\n']) {
            return Err(RepoNyxError::new(
                ErrorCode::InvalidInput,
                "Credential must not contain CR or LF",
            ));
        }
        Ok(Self { username, password })
    }

    /// The password. Only the credential helper may use it, to set the
    /// child process environment.
    fn password(&self) -> &str {
        &self.password
    }
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// A short-lived credential helper: a temporary directory holding a
/// script that serves one credential to Git. Dropping this struct
/// deletes the directory and the script.
///
/// Keep the helper alive (hold the `Arc`) until every Git command
/// using it has finished — deleting the script mid-operation makes
/// Git's credential lookup fail.
pub struct CredentialHelper {
    _dir: tempfile::TempDir,
    script: PathBuf,
    username: String,
    password: String,
}

impl CredentialHelper {
    /// Create a helper for `credential`. The script contains no secret
    /// — it reads the credential from the child process environment.
    pub fn create(credential: &Credential) -> Result<Arc<Self>, RepoNyxError> {
        let dir = tempfile::tempdir().map_err(|e| {
            RepoNyxError::new(
                ErrorCode::InternalError,
                "Cannot create credential helper dir",
            )
            .with_details(e.to_string())
        })?;
        let script_name = if cfg!(windows) {
            "reponyx-credential-helper.bat"
        } else {
            "reponyx-credential-helper.sh"
        };
        let script = dir.path().join(script_name);
        std::fs::write(&script, helper_script()).map_err(|e| {
            RepoNyxError::new(
                ErrorCode::InternalError,
                "Cannot write credential helper script",
            )
            .with_details(e.to_string())
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).map_err(
                |e| {
                    RepoNyxError::new(
                        ErrorCode::InternalError,
                        "Cannot set credential helper permissions",
                    )
                    .with_details(e.to_string())
                },
            )?;
        }
        Ok(Arc::new(Self {
            _dir: dir,
            script,
            username: credential.username.clone(),
            password: credential.password().to_string(),
        }))
    }

    /// Path of the helper script. For diagnostics only — never log the
    /// script's contents (it contains no secret, but there is no reason
    /// to print it).
    pub fn script_path(&self) -> &Path {
        &self.script
    }

    /// Install this helper on a Git command: prepend
    /// `-c credential.helper=<script>` (before the subcommand, so it
    /// applies to the whole invocation), set the credential
    /// environment variables, and disable interactive prompting.
    /// The token never appears in argv.
    pub fn install(&self, cmd: &mut GitCommand) {
        let helper_arg = format!("credential.helper={}", self.script.to_string_lossy());
        cmd.args.insert(0, helper_arg);
        cmd.args.insert(0, "-c".to_string());
        cmd.env
            .push((CREDENTIAL_USERNAME_ENV.to_string(), self.username.clone()));
        cmd.env
            .push((CREDENTIAL_PASSWORD_ENV.to_string(), self.password.clone()));
        cmd.env
            .push(("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()));
    }
}

impl std::fmt::Debug for CredentialHelper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialHelper")
            .field("script", &self.script)
            .finish()
    }
}

/// The helper script. It answers `get` from the environment and
/// ignores `store` / `erase` (Git may call those after auth; there is
/// nothing to persist — the OS credential store owns the secret).
fn helper_script() -> &'static str {
    if cfg!(windows) {
        "@echo off\r\n\
         REM RepoNyx short-lived credential helper. The credential is\r\n\
         REM never written to this file: it comes from the environment.\r\n\
         if \"%~1\"==\"get\" (\r\n\
         \u{20}echo username=%REPONYX_GIT_CREDENTIAL_USERNAME%\r\n\
         \u{20}echo password=%REPONYX_GIT_CREDENTIAL_PASSWORD%\r\n\
         )\r\n"
    } else {
        "#!/bin/sh\n\
         # RepoNyx short-lived credential helper. The credential is\n\
         # never written to this file: it comes from the environment.\n\
         if [ \"$1\" = \"get\" ]; then\n\
         \u{20} printf 'username=%s\\n' \"$REPONYX_GIT_CREDENTIAL_USERNAME\"\n\
         \u{20} printf 'password=%s\\n' \"$REPONYX_GIT_CREDENTIAL_PASSWORD\"\n\
         fi\n\
         exit 0\n"
    }
}

/// Reject a URL carrying embedded credentials, e.g.
/// `https://user:password@github.com/owner/repo.git`. A username-only
/// authority (`git@github.com`) is fine — SSH carries no password in
/// the URL. Scp-like syntax (`git@host:path`) has no `://` and is not
/// checked here.
pub fn reject_embedded_credentials(url: &str) -> Result<(), RepoNyxError> {
    if let Some(after_scheme) = url.split("://").nth(1) {
        let authority = after_scheme.split('/').next().unwrap_or("");
        if let Some(at) = authority.rfind('@') {
            let userinfo = &authority[..at];
            if userinfo.contains(':') {
                // Never echo the authority back: it contains the password.
                return Err(RepoNyxError::new(
                    ErrorCode::InvalidInput,
                    "URL contains embedded credentials; pass them via a credential helper instead",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::build_ls_remote;
    use std::path::PathBuf;

    #[test]
    fn helper_script_contains_no_secret() {
        let credential = Credential::new("octo-user", "s3cr3t-token-value").unwrap();
        let helper = CredentialHelper::create(&credential).unwrap();
        let script = std::fs::read_to_string(helper.script_path()).unwrap();
        assert!(!script.contains("s3cr3t-token-value"));
        assert!(!script.contains("octo-user"));
        assert!(script.contains(CREDENTIAL_PASSWORD_ENV));
        assert!(script.contains(CREDENTIAL_USERNAME_ENV));
    }

    #[test]
    fn install_puts_nothing_secret_in_argv() {
        let credential = Credential::new("octo-user", "s3cr3t-token-value").unwrap();
        let helper = CredentialHelper::create(&credential).unwrap();
        let mut cmd = build_ls_remote(&PathBuf::from("git"), "https://github.com/o/r.git");
        helper.install(&mut cmd);
        let argv = cmd.argv().join(" ");
        assert!(!argv.contains("s3cr3t-token-value"));
        assert!(argv.starts_with("git -c credential.helper="));
        assert_eq!(
            cmd.env
                .iter()
                .find(|(k, _)| k == CREDENTIAL_PASSWORD_ENV)
                .map(|(_, v)| v.as_str()),
            Some("s3cr3t-token-value")
        );
        assert_eq!(
            cmd.env
                .iter()
                .find(|(k, _)| k == "GIT_TERMINAL_PROMPT")
                .map(|(_, v)| v.as_str()),
            Some("0")
        );
    }

    #[test]
    fn helper_dir_is_removed_on_drop() {
        let credential = Credential::new("u", "p").unwrap();
        let helper = CredentialHelper::create(&credential).unwrap();
        let dir = helper.script_path().parent().unwrap().to_path_buf();
        assert!(dir.exists());
        drop(helper);
        assert!(!dir.exists());
    }

    #[test]
    fn credential_debug_redacts_password() {
        let credential = Credential::new("u", "s3cr3t").unwrap();
        assert!(!format!("{credential:?}").contains("s3cr3t"));
    }

    #[test]
    fn rejects_embedded_credentials_in_urls() {
        assert!(reject_embedded_credentials("https://github.com/o/r.git").is_ok());
        assert!(reject_embedded_credentials("https://user@github.com/o/r.git").is_ok());
        assert!(reject_embedded_credentials("git@github.com:o/r.git").is_ok());
        assert!(reject_embedded_credentials("https://user:pass@github.com/o/r.git").is_err());
        assert!(reject_embedded_credentials("https://:pass@github.com/o/r.git").is_err());
    }

    #[test]
    fn rejects_credentials_with_newlines() {
        assert!(Credential::new("user\nname", "pass").is_err());
        assert!(Credential::new("user", "pa\nss").is_err());
        assert!(Credential::new("", "pass").is_err());
        assert!(Credential::new("user", "").is_err());
    }
}
