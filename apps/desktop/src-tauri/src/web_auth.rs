//! Tauri commands for the web-platform login (plan §4, W3).
//!
//! The desktop talks to the web (`https://reponyx.bond`) over HTTPS:
//! email login, the 2FA step, the desktop OAuth exchange, session
//! validation, device registration, and logout. The session token
//! lives only in the OS credential store
//! (`reponyx:web` / `session`) — never in SQLite, frontend state,
//! logs, or an error message. The frontend never sees the token:
//! these commands return the user and the package, never the token.
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. They run on a machine where the Tauri
//! build works, and the CI windows-build job compiles this crate.

use crate::auth::{
    AuthState, CommandError, CommandResult, device_identifier, device_name, invalid_input,
    require_db,
};
use reponyx_credential_store::{ACCOUNT_WEB_SESSION, SERVICE_WEB};
use reponyx_database::Database;
use reponyx_database::settings::{get_setting, set_setting};
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use std::process::Command;
use std::time::Duration;
use tauri::State;

/// The web's base URL. `REPONYX_WEB_URL` overrides it for local
/// development (a dev server on localhost); production is always
/// `https://reponyx.bond`. Shared with the broker spawn config —
/// the broker must talk to the same web the desktop logged into.
pub(crate) fn web_base_url() -> String {
    std::env::var("REPONYX_WEB_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "https://reponyx.bond".to_string())
}

fn web_client() -> CommandResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| CommandError {
            code: "WebUnreachable".to_string(),
            message: "could not start the web request — try again".to_string(),
            retry_at: None,
        })
}

// -- DTOs (camelCase, matching the TypeScript types in `packages/shared`) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUserDto {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPackageDto {
    pub name: String,
    pub max_linked_accounts: i64,
    pub max_devices: i64,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUsageDto {
    pub linked_accounts: i64,
    pub devices: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebAccountDto {
    pub user: WebUserDto,
    pub package: WebPackageDto,
    pub usage: WebUsageDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSessionStatusDto {
    pub authenticated: bool,
    /// The web could not be reached, so the stored session is trusted
    /// locally (plan §4: an already-logged-in session keeps working
    /// offline) and the account below is the last-known one.
    pub offline: bool,
    /// The web answered 401: the session is dead and was deleted.
    pub session_expired: bool,
    pub account: Option<WebAccountDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebLoginDto {
    /// `"ok"` or `"2fa_required"`.
    pub step: String,
    /// Present only for the 2FA step: proves the password step passed.
    pub pending_token: Option<String>,
    /// Present once the login completed (never the session token).
    pub user: Option<WebUserDto>,
}

// -- Web error shape -----------------------------------------------------

#[derive(Debug, Deserialize)]
struct WebErrorBody {
    error: WebError,
}

#[derive(Debug, Deserialize)]
struct WebError {
    code: String,
    data: Option<serde_json::Value>,
}

/// The web's error code becomes the command error code, so the
/// frontend localizes it from `errors.json`; the message is the
/// plain-English fallback for codes the frontend does not know.
fn web_error(body: &WebErrorBody) -> CommandError {
    let message = match body.error.code.as_str() {
        "invalid_credentials" => "Email or password is incorrect",
        "email_not_verified" => "Please verify your email first",
        "account_switch_blocked" => {
            "You logged out of a different account recently — try again later"
        }
        "invalid_2fa_code" => "The code is incorrect — try again",
        "invalid_token" => "This link is invalid or expired",
        "oauth_no_account" => "No account found — please register on the web first",
        "email_taken" => "An account with this email already exists",
        "oauth_failed" => "Could not sign you in with that provider — try again",
        "not_configured" => "The service is not configured yet",
        "installation_not_linked" => "That GitHub App installation is not linked to your account",
        "device_revoked" => "This device was revoked — pair it again to sync",
        "limit_exceeded" => {
            "You have reached your package limit for linked accounts — upgrade or unlink an account"
        }
        "already_linked" => "That account is already linked to another RepoNyx account",
        _ => "Something went wrong — try again",
    };
    let retry_at = body
        .error
        .data
        .as_ref()
        .and_then(|data| data.get("retry_at"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    CommandError {
        code: body.error.code.clone(),
        message: message.to_string(),
        retry_at,
    }
}

fn unreachable() -> CommandError {
    CommandError {
        code: "WebUnreachable".to_string(),
        message: "Could not reach reponyx.bond — check your connection and try again".to_string(),
        retry_at: None,
    }
}

// -- Session + cached account (OS store + settings table) ----------------

fn store_session(auth: &AuthState, token: &str) -> CommandResult<()> {
    auth.store
        .store(SERVICE_WEB, ACCOUNT_WEB_SESSION, token)
        .map_err(CommandError::from)
}

fn read_session(auth: &AuthState) -> CommandResult<Option<String>> {
    Ok(auth
        .store
        .get(SERVICE_WEB, ACCOUNT_WEB_SESSION)
        .map_err(CommandError::from)?
        .map(|secret| secret.expose().to_string()))
}

fn clear_session(auth: &AuthState) -> CommandResult<()> {
    auth.store
        .delete(SERVICE_WEB, ACCOUNT_WEB_SESSION)
        .map_err(CommandError::from)
}

/// Settings key for the last-known account (the `/api/me/limits`
/// payload). Internal state, like `device_identifier` — not a
/// writable setting, so the offline status check can still show
/// who is signed in.
const CACHED_ACCOUNT_KEY: &str = "web.account";

async fn cache_account(database: &Database, account: &WebAccountDto) -> CommandResult<()> {
    let json = serde_json::to_string(account).map_err(|_| CommandError {
        code: "InternalError".to_string(),
        message: "could not cache the account".to_string(),
        retry_at: None,
    })?;
    set_setting(&database.pool, CACHED_ACCOUNT_KEY, &json)
        .await
        .map(|_| ())
        .map_err(CommandError::from)
}

async fn read_cached_account(database: &Database) -> CommandResult<Option<WebAccountDto>> {
    let Some(setting) = get_setting(&database.pool, CACHED_ACCOUNT_KEY)
        .await
        .map_err(CommandError::from)?
    else {
        return Ok(None);
    };
    Ok(serde_json::from_str(&setting.value).ok())
}

async fn clear_cached_account(database: &Database) -> CommandResult<()> {
    reponyx_database::settings::delete_setting(&database.pool, CACHED_ACCOUNT_KEY)
        .await
        .map(|_| ())
        .map_err(CommandError::from)
}

// -- Device info sent to the web -----------------------------------------

struct DeviceInfo {
    device_key: String,
    machine_name: String,
    platform: String,
    app_version: String,
}

async fn device_info(database: &Database) -> CommandResult<DeviceInfo> {
    Ok(DeviceInfo {
        device_key: device_identifier(database).await?,
        machine_name: device_name(),
        platform: windows_version(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// The Windows version string the web shows for this device
/// ("Windows 11 Pro 24H2"). Read from the registry the same way
/// the autostart checks read it (`reg query`, hidden console, so
/// no window flashes). Falls back to the OS name when the registry
/// read fails, and to the compile-time OS on other platforms.
fn windows_version() -> String {
    #[cfg(target_os = "windows")]
    {
        let product = reg_value("ProductName");
        let display = reg_value("DisplayVersion");
        let build = reg_value("CurrentBuildNumber");
        let mut parts: Vec<String> = Vec::new();
        if let Some(product) = product {
            parts.push(product);
        }
        if let Some(display) = display {
            parts.push(display);
        }
        if let Some(build) = build {
            parts.push(format!("({build})"));
        }
        if parts.is_empty() {
            "Windows".to_string()
        } else {
            parts.join(" ")
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::consts::OS.to_string()
    }
}

#[cfg(target_os = "windows")]
fn reg_value(name: &str) -> Option<String> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "/v",
            name,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // `    DisplayVersion    REG_SZ    24H2` — the value is the last
    // whitespace-separated field; the name is the field before it.
    let line = text.lines().find(|line| line.contains(name))?;
    let fields: Vec<&str> = line.split_whitespace().collect();
    let value = fields.last()?;
    if *value == name {
        return None;
    }
    Some((*value).to_string())
}

// -- Web calls -----------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceBody {
    device_key: String,
    machine_name: String,
    platform: String,
    app_version: String,
}

impl DeviceBody {
    fn new(info: &DeviceInfo) -> Self {
        Self {
            device_key: info.device_key.clone(),
            machine_name: info.machine_name.clone(),
            platform: info.platform.clone(),
            app_version: info.app_version.clone(),
        }
    }
}

/// Request bodies for the login-like POSTs: the credentials plus the
/// device the web registers (or refreshes) for this login.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginBody {
    email: String,
    password: String,
    #[serde(flatten)]
    device: DeviceBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Verify2faBody {
    token: String,
    code: String,
    #[serde(flatten)]
    device: DeviceBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeBody {
    code: String,
    #[serde(flatten)]
    device: DeviceBody,
}

async fn fetch_account(
    client: &reqwest::Client,
    base: &str,
    token: &str,
) -> Result<WebAccountDto, CommandError> {
    let res = client
        .get(format!("{base}/api/me/limits"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| unreachable())?;
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(CommandError {
            code: "unauthorized".to_string(),
            message: "Your session expired — please log in again".to_string(),
            retry_at: None,
        });
    }
    if !res.status().is_success() {
        let body: WebErrorBody = res.json().await.map_err(|_| unreachable())?;
        return Err(web_error(&body));
    }
    res.json::<WebAccountDto>().await.map_err(|_| unreachable())
}

/// Refresh last-seen on the web for this device. Best-effort: a
/// failed heartbeat never fails the login or the status check.
async fn heartbeat(client: &reqwest::Client, base: &str, token: &str, info: &DeviceInfo) {
    let _ = client
        .post(format!("{base}/api/devices"))
        .bearer_auth(token)
        .json(&DeviceBody::new(info))
        .send()
        .await;
}

async fn finish_login(
    auth: &AuthState,
    database: &Database,
    token: String,
    user: WebUserDto,
) -> CommandResult<WebLoginDto> {
    store_session(auth, &token)?;
    let client = web_client()?;
    let base = web_base_url();
    // The account (package + limits) is fetched right away so the
    // Settings page can show it without another round trip; a
    // failure here is not fatal — the status check fetches it next.
    if let Ok(account) = fetch_account(&client, &base, &token).await {
        let _ = cache_account(database, &account).await;
    }
    let info = device_info(database).await?;
    heartbeat(&client, &base, &token, &info).await;
    Ok(WebLoginDto {
        step: "ok".to_string(),
        pending_token: None,
        user: Some(user),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginOkResponse {
    user: WebUserDto,
    session: SessionResponse,
}

async fn post_login_like(
    client: &reqwest::Client,
    base: &str,
    path: &str,
    body: &impl Serialize,
) -> Result<serde_json::Value, CommandError> {
    let res = client
        .post(format!("{base}{path}"))
        .json(body)
        .send()
        .await
        .map_err(|_| unreachable())?;
    let status = res.status();
    let json: serde_json::Value = res.json().await.map_err(|_| unreachable())?;
    if !status.is_success() {
        let body: WebErrorBody = serde_json::from_value(json).map_err(|_| unreachable())?;
        return Err(web_error(&body));
    }
    Ok(json)
}

// -- Commands ------------------------------------------------------------

/// Email + password login. On success the session token goes to the
/// OS store and the user comes back (never the token). When the
/// account has 2FA on, the step is `2fa_required` with the pending
/// token — the frontend shows the code field and calls
/// `web_verify_2fa_command`.
#[tauri::command]
pub async fn web_login_command(
    auth: State<'_, AuthState>,
    db: State<'_, Option<Database>>,
    email: String,
    password: String,
) -> CommandResult<WebLoginDto> {
    let database = require_db(&db)?;
    let client = web_client()?;
    let base = web_base_url();
    let info = device_info(database).await?;
    let body = LoginBody {
        email,
        password,
        device: DeviceBody::new(&info),
    };
    let json = post_login_like(&client, &base, "/api/auth/login", &body).await?;
    if json.get("step").and_then(|step| step.as_str()) == Some("2fa_required") {
        let pending_token = json
            .get("pendingToken")
            .and_then(|token| token.as_str())
            .map(str::to_string);
        return Ok(WebLoginDto {
            step: "2fa_required".to_string(),
            pending_token,
            user: None,
        });
    }
    let ok: LoginOkResponse = serde_json::from_value(json).map_err(|_| unreachable())?;
    finish_login(&auth, database, ok.session.token, ok.user).await
}

/// Complete a login that stopped at the 2FA step.
#[tauri::command]
pub async fn web_verify_2fa_command(
    auth: State<'_, AuthState>,
    db: State<'_, Option<Database>>,
    pending_token: String,
    code: String,
) -> CommandResult<WebLoginDto> {
    let database = require_db(&db)?;
    let client = web_client()?;
    let base = web_base_url();
    let info = device_info(database).await?;
    let body = Verify2faBody {
        token: pending_token,
        code,
        device: DeviceBody::new(&info),
    };
    let json = post_login_like(&client, &base, "/api/auth/2fa/verify", &body).await?;
    let ok: LoginOkResponse = serde_json::from_value(json).map_err(|_| unreachable())?;
    finish_login(&auth, database, ok.session.token, ok.user).await
}

/// The login gate: is there a session, and is it still valid?
/// A 401 deletes the session and reports `sessionExpired`. A
/// network failure keeps the session (offline rule) and returns
/// the last-known account.
#[tauri::command]
pub async fn web_session_status_command(
    auth: State<'_, AuthState>,
    db: State<'_, Option<Database>>,
) -> CommandResult<WebSessionStatusDto> {
    let database = require_db(&db)?;
    let Some(token) = read_session(&auth)? else {
        return Ok(WebSessionStatusDto {
            authenticated: false,
            offline: false,
            session_expired: false,
            account: None,
        });
    };
    let client = web_client()?;
    let base = web_base_url();
    match fetch_account(&client, &base, &token).await {
        Ok(account) => {
            let _ = cache_account(database, &account).await;
            let info = device_info(database).await?;
            heartbeat(&client, &base, &token, &info).await;
            Ok(WebSessionStatusDto {
                authenticated: true,
                offline: false,
                session_expired: false,
                account: Some(account),
            })
        }
        Err(err) if err.code == "unauthorized" => {
            let _ = clear_session(&auth);
            let _ = clear_cached_account(database).await;
            Ok(WebSessionStatusDto {
                authenticated: false,
                offline: false,
                session_expired: true,
                account: None,
            })
        }
        Err(err) if err.code == "WebUnreachable" => Ok(WebSessionStatusDto {
            authenticated: true,
            offline: true,
            session_expired: false,
            account: read_cached_account(database).await?,
        }),
        Err(err) => Err(err),
    }
}

/// Log out: tell the web (best-effort — a dead network must not
/// block logging out), then delete the session and the cached
/// account locally.
#[tauri::command]
pub async fn web_logout_command(
    auth: State<'_, AuthState>,
    db: State<'_, Option<Database>>,
) -> CommandResult<()> {
    let database = require_db(&db)?;
    if let Some(token) = read_session(&auth)? {
        let client = web_client()?;
        let base = web_base_url();
        let _ = client
            .post(format!("{base}/api/auth/logout"))
            .bearer_auth(token)
            .send()
            .await;
    }
    clear_session(&auth)?;
    clear_cached_account(database).await
}

/// Open the web's OAuth start for the desktop flow in the system
/// browser. The provider finishes in the browser and hands the
/// result back through `reponyx://auth`.
#[tauri::command]
pub async fn web_open_oauth_command(provider: String) -> CommandResult<()> {
    if provider != "google" && provider != "github" {
        return Err(invalid_input("provider must be google or github"));
    }
    let url = format!(
        "{}/api/auth/oauth/{provider}?client=desktop",
        web_base_url()
    );
    crate::repositories::open_with_os_app(&url)
}

/// Open the web's register page in the system browser. Registration
/// happens on the web only — the desktop never creates accounts.
#[tauri::command]
pub async fn web_open_register_command() -> CommandResult<()> {
    crate::repositories::open_with_os_app(&format!("{}/register", web_base_url()))
}

/// Open the web dashboard ("Manage on the web") in the system browser.
#[tauri::command]
pub async fn web_open_dashboard_command() -> CommandResult<()> {
    crate::repositories::open_with_os_app(&format!("{}/dashboard", web_base_url()))
}

/// Open a web-platform URL in the system browser (e.g. the
/// `/register/complete?token=…` page after an OAuth registration
/// that still needs the user's name). A path starting with `/` is
/// resolved against the web's base URL; an absolute URL must be
/// under that base URL. This is not a general-purpose opener.
#[tauri::command]
pub async fn web_open_url_command(url: String) -> CommandResult<()> {
    let base = web_base_url();
    let full = if let Some(path) = url.strip_prefix('/') {
        format!("{base}/{path}")
    } else if url == base || url.starts_with(&format!("{base}/")) {
        url
    } else {
        return Err(invalid_input("url must be under the web's base URL"));
    };
    crate::repositories::open_with_os_app(&full)
}

/// Exchange a desktop auth code from the `reponyx://auth` deep link
/// for a session token.
#[tauri::command]
pub async fn web_exchange_desktop_code_command(
    auth: State<'_, AuthState>,
    db: State<'_, Option<Database>>,
    code: String,
) -> CommandResult<WebLoginDto> {
    let database = require_db(&db)?;
    let client = web_client()?;
    let base = web_base_url();
    let info = device_info(database).await?;
    let body = ExchangeBody {
        code,
        device: DeviceBody::new(&info),
    };
    let json = post_login_like(&client, &base, "/api/auth/desktop/exchange", &body).await?;
    let ok: LoginOkResponse = serde_json::from_value(json).map_err(|_| unreachable())?;
    finish_login(&auth, database, ok.session.token, ok.user).await
}

// -- Linked GitHub accounts (W4) ----------------------------------------

/// A linked GitHub account as the web returns it (the POST
/// `/api/me/github-accounts` response's `account` row, mapped to
/// camelCase). The id is the web row id — the unlink command
/// needs it, and the frontend never has to guess it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebLinkedAccountDto {
    pub id: String,
    pub login: String,
    pub auth_kind: String,
}

#[derive(Debug, Deserialize)]
struct LinkedAccountResponse {
    account: LinkedAccountRow,
}

/// The web's `linked_github_accounts` row (snake_case on the
/// wire); fields the desktop does not need are ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct LinkedAccountRow {
    id: String,
    login: String,
    auth_kind: String,
}

fn require_session(auth: &AuthState) -> CommandResult<String> {
    read_session(auth)?.ok_or_else(|| CommandError {
        code: "unauthorized".to_string(),
        message: "Your session expired — please log in again".to_string(),
        retry_at: None,
    })
}

/// A web call with the session as a Bearer token. `body` is the
/// JSON request body for a POST (None for DELETE). A non-2xx
/// response becomes the web's error code via [`web_error`].
async fn authed_web_call(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value, CommandError> {
    let mut request = match method {
        "POST" => client.post(format!("{base}{path}")),
        "DELETE" => client.delete(format!("{base}{path}")),
        _ => client.get(format!("{base}{path}")),
    }
    .bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let res = request.send().await.map_err(|_| unreachable())?;
    let status = res.status();
    let json: serde_json::Value = res.json().await.map_err(|_| unreachable())?;
    if !status.is_success() {
        let body: WebErrorBody = serde_json::from_value(json).map_err(|_| unreachable())?;
        return Err(web_error(&body));
    }
    Ok(json)
}

async fn link_github_account(
    auth: &AuthState,
    body: serde_json::Value,
) -> CommandResult<WebLinkedAccountDto> {
    let token = require_session(auth)?;
    let client = web_client()?;
    let base = web_base_url();
    let json = authed_web_call(
        &client,
        &base,
        &token,
        "POST",
        "/api/me/github-accounts",
        Some(body),
    )
    .await?;
    let response: LinkedAccountResponse =
        serde_json::from_value(json).map_err(|_| unreachable())?;
    Ok(WebLinkedAccountDto {
        id: response.account.id,
        login: response.account.login,
        auth_kind: response.account.auth_kind,
    })
}

/// Link a paired GitHub App installation to the signed-in web
/// account. Idempotent: linking an already-linked installation
/// returns it. The web enforces the package limit (403
/// `limit_exceeded`) and the one-user rule (409 `already_linked`).
#[tauri::command]
pub async fn web_link_github_account_command(
    auth: State<'_, AuthState>,
    installation_id: String,
) -> CommandResult<WebLinkedAccountDto> {
    link_github_account(
        &auth,
        serde_json::json!({ "authKind": "app", "installationId": installation_id }),
    )
    .await
}

/// Link a PAT account to the signed-in web account, by its GitHub
/// login. No token ever travels — the link records the login
/// only. Idempotent, same limits as the App link.
#[tauri::command]
pub async fn web_link_pat_account_command(
    auth: State<'_, AuthState>,
    login: String,
) -> CommandResult<WebLinkedAccountDto> {
    link_github_account(
        &auth,
        serde_json::json!({ "authKind": "pat", "login": login }),
    )
    .await
}

/// Unlink a PAT account from the signed-in web account, by its
/// GitHub login. The web's DELETE takes the row id, so this
/// re-links idempotently first to learn it, then deletes it. A
/// link that fails with `limit_exceeded` means the account was
/// never linked (a linked one returns the existing row without
/// a limit check), and `already_linked` means the link belongs
/// to another user — either way there is nothing of ours to
/// delete, so both return Ok.
#[tauri::command]
pub async fn web_unlink_pat_account_command(
    auth: State<'_, AuthState>,
    login: String,
) -> CommandResult<()> {
    let linked = match link_github_account(
        &auth,
        serde_json::json!({ "authKind": "pat", "login": login }),
    )
    .await
    {
        Ok(linked) => linked,
        Err(err) if err.code == "limit_exceeded" || err.code == "already_linked" => {
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    let token = require_session(&auth)?;
    let client = web_client()?;
    let base = web_base_url();
    authed_web_call(
        &client,
        &base,
        &token,
        "DELETE",
        &format!("/api/me/github-accounts/{}", linked.id),
        None,
    )
    .await?;
    Ok(())
}
