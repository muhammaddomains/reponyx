//! Desktop-side Auth Broker lifecycle: resolve + spawn the broker
//! binary, track the child the desktop owns, stop it on quit.
//!
//! The broker is a separate process the desktop starts on demand —
//! at startup, and (best-effort) before any broker command — so the
//! Accounts page never fails just because the broker was not
//! running. The desktop only ever stops a broker it spawned itself;
//! a broker the user started by hand (or via `dev:all`) is left
//! alone on quit.
//!
//! Broker configuration: in dev the spawned broker runs with the
//! repo root as its working directory, so it reads the same `.env`
//! and the same relative `broker-data` dir as `pnpm broker`. For an
//! installed app there is no repo root: the broker runs with the
//! data dir as its working directory and its state in
//! `<data_dir>/broker-data` — and its configuration comes from the
//! desktop itself, not from a hand-made file. The GitHub App id and
//! the private-key *path* are ordinary settings (Accounts → Auth
//! Broker). The setup token is never typed and never shown: it
//! comes from a broker `.env` file when one exists (dev, or a
//! hand-made file), and otherwise the desktop generates one per
//! install, stores it in the OS credential store, and passes it to
//! the broker it spawns as an environment variable. It is
//! deliberately NOT baked into the build: a token in the binary
//! would be the same for every user and readable by anyone with
//! the installer, so anyone could pair a device to anyone's local
//! broker. The private key itself never passes through the
//! desktop — only where the file is. A `<data_dir>/.env` file, when
//! the user made one, still works as an override: explicit
//! variables win over it, because dotenv never overwrites a
//! variable that is already set.

use crate::auth::AuthState;
use crate::exe;
use reponyx_core::SecretString;
use reponyx_database::Database;
use reponyx_database::settings::get_setting;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The broker child this desktop process spawned (if any), held so
/// quit can stop exactly what it started — and nothing else.
pub struct BrokerProcess(pub Mutex<Option<Child>>);

/// What the broker needs to start, resolved from the desktop's own
/// stores: the GitHub App id and the private-key *path* from
/// settings, the setup token from a broker `.env` file when one
/// exists, otherwise generated per install and kept in the OS
/// credential store (see [`crate::auth::ensure_broker_setup_token`]).
/// The key itself is never here — only where its file is.
#[derive(Debug, Default)]
pub struct BrokerSpawnConfig {
    pub app_id: Option<String>,
    pub private_key_path: Option<String>,
    pub setup_token: Option<SecretString>,
}

impl BrokerSpawnConfig {
    /// Whether the broker has everything it needs to start. A
    /// `.env` file (repo checkout in dev, or a hand-made
    /// `<data_dir>/.env`) can supply all three values on its own;
    /// without one, the App id and the key path must be present —
    /// the setup token is not required here, it is generated per
    /// install when the config resolves (see
    /// [`crate::auth::ensure_broker_setup_token`]).
    pub fn is_complete(&self, env_file_present: bool) -> bool {
        env_file_present || (self.app_id.is_some() && self.private_key_path.is_some())
    }

    /// Environment variables for the spawned broker. Explicit
    /// variables win over any `.env` file, because dotenv never
    /// overwrites a variable that is already set — so settings the
    /// user saved in the app always beat a stale file.
    pub fn env_vars(&self) -> Vec<(String, String)> {
        let mut vars = Vec::new();
        if let Some(app_id) = &self.app_id {
            vars.push(("REPONYX_BROKER_APP_ID".to_string(), app_id.clone()));
        }
        if let Some(path) = &self.private_key_path {
            vars.push(("REPONYX_BROKER_PRIVATE_KEY_PATH".to_string(), path.clone()));
        }
        if let Some(token) = &self.setup_token {
            vars.push((
                "REPONYX_BROKER_SETUP_TOKEN".to_string(),
                token.expose().to_string(),
            ));
        }
        vars
    }
}

/// Resolve the broker configuration from the desktop's stores.
/// Missing values stay `None` — the caller reports the gap instead
/// of spawning a broker that would exit at once. The setup token
/// is the exception: it resolves itself (`.env` file, else the OS
/// store, else generated per install), so a resolved config that
/// is complete always carries one.
async fn resolve_spawn_config(app: &AppHandle) -> BrokerSpawnConfig {
    let mut config = BrokerSpawnConfig::default();
    if let Some(database) = app.state::<Option<Database>>().as_ref() {
        if let Ok(Some(setting)) = get_setting(&database.pool, "broker.app_id").await {
            if !setting.value.trim().is_empty() {
                config.app_id = Some(setting.value);
            }
        }
        if let Ok(Some(setting)) = get_setting(&database.pool, "broker.private_key_path").await {
            if !setting.value.trim().is_empty() {
                config.private_key_path = Some(setting.value);
            }
        }
    }
    config.setup_token = crate::auth::ensure_broker_setup_token(&app.state::<AuthState>()).ok();
    config
}

/// The setup token from a broker `.env` file, if one exists and
/// names a usable one: the data-dir file first (an installed app's
/// hand-made file), then the repo checkout's `.env` in dev. Only
/// the token is read — never the private key: a `.env` holding the
/// key itself belongs to a broker the user started by hand, and the
/// desktop never touches key material.
pub(crate) fn setup_token_from_env_file() -> Option<SecretString> {
    let mut candidates = vec![data_env_file()];
    if let Some(root) = exe::find_repo_root_with_env() {
        candidates.push(root.join(".env"));
    }
    candidates
        .iter()
        .find_map(|path| read_setup_token_from_dotenv(path))
}

/// The `REPONYX_BROKER_SETUP_TOKEN` line of one `.env` file.
/// `export ` prefixes and single/double quotes are tolerated; a
/// value shorter than the broker's 16-character minimum counts as
/// absent (the broker would refuse it, and the desktop's generated
/// token overrides it anyway).
fn read_setup_token_from_dotenv(path: &std::path::Path) -> Option<SecretString> {
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "REPONYX_BROKER_SETUP_TOKEN" {
            continue;
        }
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        if value.len() >= 16 {
            return Some(SecretString::new(value));
        }
    }
    None
}

/// Whether a broker `.env` file exists that could supply the
/// broker's configuration on its own: the repo checkout's `.env` in
/// dev, or a hand-made `<data_dir>/.env` for an installed app.
pub fn broker_env_file_present() -> bool {
    exe::find_repo_root_with_env().is_some() || data_env_file().is_file()
}

fn data_env_file() -> PathBuf {
    crate::agent::data_dir().join(".env")
}

fn broker_base_url(app: &AppHandle) -> String {
    app.state::<AuthState>().inner().broker_base_url.clone()
}

fn socket_addr(base_url: &str) -> Option<SocketAddr> {
    let host = base_url
        .strip_prefix("http://")
        .or_else(|| base_url.strip_prefix("https://"))?;
    let host = host.split('/').next()?;
    host.parse().ok()
}

/// Is the broker answering `/health`? A bare TCP connect is not
/// enough — another process could hold the port — so this sends a
/// minimal HTTP GET and requires a 200 status line. Short timeout:
/// this runs on the Accounts page's refresh path.
pub async fn broker_reachable(app: &AppHandle) -> bool {
    let Some(addr) = socket_addr(&broker_base_url(app)) else {
        return false;
    };
    let connect =
        tokio::time::timeout(Duration::from_secs(1), tokio::net::TcpStream::connect(addr)).await;
    let Ok(Ok(mut stream)) = connect else {
        return false;
    };
    let request = format!("GET /health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).await.is_err() {
        return false;
    }
    let mut buf = [0u8; 256];
    let read = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buf)).await;
    let Ok(Ok(n)) = read else {
        return false;
    };
    std::str::from_utf8(&buf[..n])
        .map(|text| text.contains(" 200 "))
        .unwrap_or(false)
}

fn resolve_broker_exe(app: Option<&AppHandle>) -> Option<PathBuf> {
    exe::resolve_exe(
        app,
        "REPONYX_BROKER_EXE",
        &exe::exe_name("reponyx-auth-broker"),
    )
}

fn spawn_broker_process(
    app: Option<&AppHandle>,
    config: &BrokerSpawnConfig,
) -> Result<Child, String> {
    let exe = resolve_broker_exe(app).ok_or_else(|| {
        "reponyx-auth-broker executable not found (set REPONYX_BROKER_EXE or install the broker binary next to the desktop app)".to_string()
    })?;
    let mut cmd = Command::new(&exe);
    // Working directory + env: dev parity first. When the desktop
    // runs from a repo checkout, the broker inherits the repo root
    // as its CWD, so it reads the repo `.env` and the relative
    // `broker-data` dir — exactly like `pnpm broker`. Otherwise
    // (installed app) the data dir is the CWD, the broker state
    // lives in `<data_dir>/broker-data`, and the configuration
    // comes from the desktop's own stores (see `BrokerSpawnConfig`).
    // A hand-made `<data_dir>/.env` still works as an override.
    match exe::find_repo_root_with_env() {
        Some(root) => {
            cmd.current_dir(root);
        }
        None => {
            let data_dir = crate::agent::data_dir();
            cmd.current_dir(&data_dir);
            cmd.env("REPONYX_BROKER_DATA_DIR", data_dir.join("broker-data"));
            if data_env_file().is_file() {
                cmd.env("REPONYX_BROKER_ENV_FILE", data_env_file());
            }
            for (key, value) in config.env_vars() {
                cmd.env(key, value);
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    #[cfg(unix)]
    {
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
    }
    cmd.spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", exe.display()))
}

/// Start the broker if it is not already reachable. Returns `true`
/// when the broker answers `/health` afterwards. A missing binary
/// is not an error here — the user may run the broker by hand — it
/// just means "not started by the desktop". A broker that cannot
/// start because its configuration is missing *is* an error, with
/// the setup page named, instead of a spawn that would exit at
/// once and surface as a bare "unreachable".
pub async fn ensure_broker_running(app: &AppHandle) -> Result<bool, String> {
    if broker_reachable(app).await {
        return Ok(true);
    }
    let config = resolve_spawn_config(app).await;
    if !config.is_complete(broker_env_file_present()) {
        return Err("Auth Broker is not configured: open Accounts → GitHub App (Auth Broker) and enter the GitHub App ID and the private-key file. The setup token is created automatically. No .env file is needed."
            .to_string());
    }
    let child = spawn_broker_process(Some(app), &config)?;
    // Keep the child so quit can stop it. A spawn that died at once
    // (e.g. missing env vars) is detected below by the health poll.
    let state = app.state::<BrokerProcess>();
    *state
        .0
        .lock()
        .map_err(|e| format!("broker state poisoned: {e}"))? = Some(child);
    // Wait briefly for /health to come up.
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if broker_reachable(app).await {
            return Ok(true);
        }
        // If the spawned broker died on startup, say so instead of
        // waiting out the clock.
        let exited = {
            let mut guard = state
                .0
                .lock()
                .map_err(|e| format!("broker state poisoned: {e}"))?;
            match guard.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        *guard = None;
                        Some(status)
                    }
                    _ => None,
                },
                None => None,
            }
        };
        if let Some(status) = exited {
            return Err(format!(
                "broker process exited during startup ({status}) — check the broker configuration (REPONYX_BROKER_APP_ID / private key / setup token)"
            ));
        }
    }
    Ok(false)
}

/// Best-effort ensure for startup and broker commands: failures
/// are logged, never raised — the caller reports the broker as
/// unreachable instead of breaking the page.
pub async fn ensure_broker_best_effort(app: &AppHandle) {
    match ensure_broker_running(app).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!("auth broker is not running and could not be started");
        }
        Err(e) => {
            tracing::warn!(error = %e, "could not start the auth broker");
        }
    }
}

/// Stop the broker this desktop process spawned, if any. Returns
/// `true` when a broker child was stopped. A broker started by hand
/// (or by dev:all) is never touched.
pub async fn stop_broker_if_spawned(app: &AppHandle) -> bool {
    let mut child = {
        let state = app.state::<BrokerProcess>();
        let Ok(mut guard) = state.0.lock() else {
            return false;
        };
        match guard.take() {
            Some(child) => child,
            None => return false,
        }
    };
    if let Err(e) = child.kill() {
        tracing::warn!(error = %e, "could not stop the spawned auth broker");
        return false;
    }
    // The child was just killed, so this returns as soon as the OS
    // reaps it.
    match child.wait() {
        Ok(_) => {
            tracing::info!("auth broker stopped");
            true
        }
        Err(e) => {
            tracing::warn!(error = %e, "spawned auth broker did not exit after kill");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_config() -> BrokerSpawnConfig {
        BrokerSpawnConfig {
            app_id: Some("5896712".to_string()),
            private_key_path: Some("C:\\keys\\reponyx.pem".to_string()),
            setup_token: Some(SecretString::new("a".repeat(32))),
        }
    }

    #[test]
    fn spawn_config_needs_app_id_and_key_without_an_env_file() {
        assert!(full_config().is_complete(false));
        let mut missing_id = full_config();
        missing_id.app_id = None;
        assert!(!missing_id.is_complete(false));
        let mut missing_key = full_config();
        missing_key.private_key_path = None;
        assert!(!missing_key.is_complete(false));
        // The setup token is NOT required here: it resolves itself
        // (.env file, else the OS store, else generated per install)
        // when the config resolves, so a config with the App id and
        // the key path but no token yet is already complete.
        let mut missing_token = full_config();
        missing_token.setup_token = None;
        assert!(missing_token.is_complete(false));
        // An empty config is complete only when an .env file can
        // supply everything instead.
        assert!(!BrokerSpawnConfig::default().is_complete(false));
        assert!(BrokerSpawnConfig::default().is_complete(true));
    }

    #[test]
    fn dotenv_token_parsing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(
            &path,
            "# a comment\nexport REPONYX_BROKER_APP_ID=5896712\nREPONYX_BROKER_SETUP_TOKEN=\"0123456789abcdef0123456789abcdef\"\n",
        )
        .unwrap();
        assert_eq!(
            read_setup_token_from_dotenv(&path).map(|t| t.expose().to_string()),
            Some("0123456789abcdef0123456789abcdef".to_string())
        );
        // Too short for the broker: counts as absent, so the
        // generated token takes over instead of a broken one.
        std::fs::write(&path, "REPONYX_BROKER_SETUP_TOKEN=short\n").unwrap();
        assert!(read_setup_token_from_dotenv(&path).is_none());
        // No token line at all.
        std::fs::write(&path, "REPONYX_BROKER_APP_ID=5896712\n").unwrap();
        assert!(read_setup_token_from_dotenv(&path).is_none());
    }

    #[test]
    fn spawn_env_passes_exactly_the_values_present() {
        let vars = full_config().env_vars();
        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&("REPONYX_BROKER_APP_ID".to_string(), "5896712".to_string())));
        assert!(vars.contains(&(
            "REPONYX_BROKER_PRIVATE_KEY_PATH".to_string(),
            "C:\\keys\\reponyx.pem".to_string()
        )));
        assert!(vars.contains(&("REPONYX_BROKER_SETUP_TOKEN".to_string(), "a".repeat(32))));
        assert!(BrokerSpawnConfig::default().env_vars().is_empty());
    }
}
