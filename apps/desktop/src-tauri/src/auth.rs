//! Tauri commands for the Phase 5 auth flows (PAT + GitHub App
//! pairing). These commands are thin delegates: the flows live in
//! `reponyx-credential-store` and `reponyx-github-provider`; this
//! module only adapts them to the Tauri boundary.
//!
//! Secret handling: a PAT or token arrives as a command argument and
//! is moved straight into the OS credential store (or used for one
//! validation call) — it is never written to the database, never
//! logged, and never appears in an error. Errors are
//! [`CommandError`] (`{ code, message }`); the message is passed
//! through `redact_secrets` as defence in depth.
//!
//! The desktop crate cannot be compiled on this VM (missing
//! WebViewGTK system libs — the known Tauri blocker), so these
//! commands are verified by `cargo fmt` + review here, and by the
//! frontend typecheck/build. They run on a machine where the Tauri
//! build works.

use chrono::{DateTime, Utc};
use reponyx_core::{
    Account, AccountStatus, AccountType, AuthenticationMethod, DevicePairingStatus,
    InstallationStatus, InstallationType, Provider, RepoNyxError,
};
use reponyx_credential_store::{
    ACCOUNT_SETUP_TOKEN, CredentialStore, GITHUB_API_BASE, PatExpiryStatus, PatInput,
    SERVICE_BROKER, SecretString, add_pat_account, delete_pat_account, pat_expiry_status,
    reconnect_pat_account, remove_pat, replace_pat, validate_pat_account,
};
use reponyx_database::Database;
use reponyx_database::accounts::{
    NewAccount, create_account, get_account, get_account_by_provider, list_accounts,
    mark_validated, reconnect_account,
};
use reponyx_database::devices::{
    NewDevice, create_device, get_device_by_identifier, set_pairing_status,
};
use reponyx_database::installations::{
    NewInstallation, list_installations_for_account, upsert_installation,
};
use reponyx_database::settings::{get_setting, set_setting};
use reponyx_github_provider::GitHubProvider;
use reponyx_github_provider::broker::{BrokerClient, BrokerTokenSource};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, State};
use uuid::Uuid;

/// Error shape returned to the frontend: `{ code, message }`, using
/// `RepoNyxError` codes. The message is redacted so a secret can
/// never leak into an error the UI displays.
#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<RepoNyxError> for CommandError {
    fn from(err: RepoNyxError) -> Self {
        Self {
            code: err.code.to_string(),
            message: reponyx_logging::redact_secrets(&err.message),
        }
    }
}

pub(crate) type CommandResult<T> = Result<T, CommandError>;

pub(crate) fn invalid_input(message: impl Into<String>) -> CommandError {
    CommandError {
        code: "InvalidInput".to_string(),
        message: message.into(),
    }
}

pub(crate) fn database_unavailable() -> CommandError {
    CommandError {
        code: "DatabaseError".to_string(),
        // The startup failure (e.g. a modified migration) is logged
        // with its real reason — point there, or the UI can never
        // say more than "unavailable".
        message: "database is unavailable — the startup error is in the app log (RepoNyx/logs/reponyx.log.<date>)".to_string(),
    }
}

/// Managed auth state: the credential store plus the API/broker base
/// URLs. The store is the OS backend; on a machine without one
/// (this Linux VM has no Secret Service) commands fail with
/// `CredentialStoreError` and the UI surfaces that honestly.
pub struct AuthState {
    pub store: Arc<dyn CredentialStore>,
    pub github_api_base: String,
    pub broker_base_url: String,
}

impl AuthState {
    pub fn new() -> Self {
        Self {
            store: Arc::new(reponyx_credential_store::OsCredentialStore::new()),
            github_api_base: GITHUB_API_BASE.to_string(),
            broker_base_url: reponyx_github_provider::broker::DEFAULT_BROKER_BASE_URL.to_string(),
        }
    }
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

// -- DTOs (camelCase, matching the TypeScript types in
// `packages/shared/src/types.ts`) -----------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDto {
    pub id: String,
    pub provider: String,
    pub provider_account_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub label: Option<String>,
    pub account_type: String,
    pub authentication_method: String,
    pub status: String,
    pub pat_expires_at: Option<String>,
    pub pat_expiry_status: String,
    pub pat_expiry_days_left: Option<i64>,
    pub last_validated_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Account> for AccountDto {
    fn from(account: &Account) -> Self {
        let (status, days_left) = match pat_expiry_status(account, Utc::now()) {
            PatExpiryStatus::Unknown => ("unknown".to_string(), None),
            PatExpiryStatus::Valid => ("valid".to_string(), None),
            PatExpiryStatus::ExpiringSoon { days_left } => {
                ("expiring_soon".to_string(), Some(days_left))
            }
            PatExpiryStatus::Expired => ("expired".to_string(), None),
        };
        Self {
            id: account.id.to_string(),
            provider: account.provider.as_str().to_string(),
            provider_account_id: account.provider_account_id.clone(),
            username: account.username.clone(),
            display_name: account.display_name.clone(),
            label: account.label.clone(),
            account_type: account.account_type.as_str().to_string(),
            authentication_method: account.authentication_method.as_str().to_string(),
            status: account.status.as_str().to_string(),
            pat_expires_at: account.pat_expires_at.map(|at| at.to_rfc3339()),
            pat_expiry_status: status,
            pat_expiry_days_left: days_left,
            last_validated_at: account.last_validated_at.map(|at| at.to_rfc3339()),
            created_at: account.created_at.to_rfc3339(),
            updated_at: account.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountDto {
    pub provider_account_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl From<reponyx_provider_abstractions::ProviderAccountInfo> for ProviderAccountDto {
    fn from(info: reponyx_provider_abstractions::ProviderAccountInfo) -> Self {
        Self {
            provider_account_id: info.provider_account_id,
            username: info.username,
            display_name: info.display_name,
            avatar_url: info.avatar_url,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatExpiryDto {
    pub status: String,
    pub days_left: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingDto {
    pub pairing_code: String,
    pub expires_at: Option<String>,
    pub installation_id: String,
    pub device_identifier: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerStatusDto {
    pub configured: bool,
    pub reachable: bool,
    pub base_url: String,
    /// A GitHub App id is saved in settings (`broker.app_id`).
    pub app_id_configured: bool,
    /// A private-key file path is saved in settings
    /// (`broker.private_key_path`).
    pub private_key_configured: bool,
    /// A broker `.env` file exists that can supply the broker's
    /// configuration on its own (repo checkout in dev, or a
    /// hand-made `<data_dir>/.env`). When this is true the two
    /// settings above are not needed.
    pub env_file_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerInstallationDto {
    pub installation_id: String,
    pub setup_action: String,
    pub account_login: Option<String>,
    pub installed_at: String,
    pub paired_device: Option<String>,
}

/// The one-time setup state for the GitHub App install flow: the
/// install URL to send the user to and when the `state` expires
/// (10 minutes). The raw `state` value is embedded in the install
/// URL and is not returned separately.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerSetupStateDto {
    pub install_url: String,
    pub expires_at: String,
}

// -- helpers ----------------------------------------------------------

pub(crate) fn require_db<'a>(db: &'a State<'_, Option<Database>>) -> CommandResult<&'a Database> {
    db.inner().as_ref().ok_or_else(database_unavailable)
}

fn parse_account_id(id: &str) -> CommandResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| invalid_input("invalid account id"))
}

fn parse_account_type(value: &str) -> CommandResult<AccountType> {
    match value {
        "personal" => Ok(AccountType::Personal),
        "organization" => Ok(AccountType::Organization),
        _ => Err(invalid_input(
            "account_type must be 'personal' or 'organization'",
        )),
    }
}

fn parse_expires_at(value: Option<String>) -> CommandResult<Option<DateTime<Utc>>> {
    match value {
        None => Ok(None),
        Some(text) => DateTime::parse_from_rfc3339(&text)
            .map(|parsed| Some(parsed.with_timezone(&Utc)))
            .map_err(|_| invalid_input("expires_at must be an RFC 3339 timestamp")),
    }
}

/// The stable per-machine device identifier, stored in the settings
/// table on first use (a UUID, generated once).
async fn device_identifier(database: &Database) -> CommandResult<String> {
    if let Some(setting) = get_setting(&database.pool, "device_identifier").await? {
        return Ok(setting.value);
    }
    let id = Uuid::new_v4().to_string();
    set_setting(&database.pool, "device_identifier", &id).await?;
    Ok(id)
}

fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "This device".to_string())
}

/// The broker setup token, resolved without ever asking the user:
/// a broker `.env` file first (dev, or a hand-made file — the same
/// token the broker itself runs with), else the OS credential
/// store, else a fresh per-install token generated and stored
/// here. The token is never typed and never shown: the desktop
/// passes it to the broker it spawns from the same store, and a
/// token baked into the build would be the same for every user
/// and readable from the installer. Errors only when the store
/// itself fails (generation included).
pub(crate) fn ensure_broker_setup_token(auth: &AuthState) -> CommandResult<SecretString> {
    if let Some(token) = crate::broker::setup_token_from_env_file() {
        return Ok(token);
    }
    if let Some(token) = auth
        .store
        .get(SERVICE_BROKER, ACCOUNT_SETUP_TOKEN)
        .map_err(CommandError::from)?
    {
        return Ok(token);
    }
    let token = generate_setup_token();
    auth.store
        .store(SERVICE_BROKER, ACCOUNT_SETUP_TOKEN, &token)?;
    Ok(SecretString::new(token))
}

/// The broker setup token for a broker API call (pairing, install
/// state, …). Resolves like [`ensure_broker_setup_token`], so a
/// call never fails for a token the desktop can create itself.
fn setup_token(auth: &AuthState) -> CommandResult<SecretString> {
    ensure_broker_setup_token(auth)
}

fn broker_client(auth: &AuthState) -> CommandResult<BrokerClient> {
    BrokerClient::new(&auth.broker_base_url).map_err(CommandError::from)
}

// -- PAT account commands ---------------------------------------------

#[tauri::command]
pub async fn list_accounts_command(
    db: State<'_, Option<Database>>,
) -> CommandResult<Vec<AccountDto>> {
    // Audit B5: a dead database must fail loudly, never answer
    // with a silent empty list that makes the Accounts page look
    // like the user has no accounts at all.
    let database = require_db(&db)?;
    let accounts = list_accounts(&database.pool).await?;
    Ok(accounts.iter().map(AccountDto::from).collect())
}

#[tauri::command]
pub async fn add_pat_account_command(
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    pat: String,
    label: Option<String>,
    account_type: String,
    expires_at: Option<String>,
) -> CommandResult<AccountDto> {
    let database = require_db(&db)?;
    let account_type = parse_account_type(&account_type)?;
    let expires_at = parse_expires_at(expires_at)?;
    let account = add_pat_account(
        &database.pool,
        &auth.store,
        &auth.github_api_base,
        PatInput {
            pat: SecretString::new(pat),
            label,
            account_type,
            expires_at,
        },
    )
    .await?;
    Ok(AccountDto::from(&account))
}

#[tauri::command]
pub async fn replace_pat_account_command(
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    account_id: String,
    pat: String,
    expires_at: Option<String>,
) -> CommandResult<AccountDto> {
    let database = require_db(&db)?;
    let id = parse_account_id(&account_id)?;
    let expires_at = parse_expires_at(expires_at)?;
    let account = replace_pat(
        &database.pool,
        &auth.store,
        &auth.github_api_base,
        id,
        SecretString::new(pat),
        expires_at,
    )
    .await?;
    Ok(AccountDto::from(&account))
}

#[tauri::command]
pub async fn remove_pat_account_command(
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    account_id: String,
) -> CommandResult<AccountDto> {
    let database = require_db(&db)?;
    let id = parse_account_id(&account_id)?;
    let account = remove_pat(&database.pool, &auth.store, id).await?;
    Ok(AccountDto::from(&account))
}

/// Delete a PAT account outright: the stored PAT is deleted and
/// the account row (plus its repositories, schedules, jobs and
/// history, via `ON DELETE CASCADE`) is removed. Local files on
/// disk are kept.
#[tauri::command]
pub async fn delete_pat_account_command(
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    account_id: String,
) -> CommandResult<bool> {
    let database = require_db(&db)?;
    let id = parse_account_id(&account_id)?;
    let deleted = delete_pat_account(&database.pool, &auth.store, id).await?;
    Ok(deleted)
}

#[tauri::command]
pub async fn reconnect_pat_account_command(
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    account_id: String,
) -> CommandResult<AccountDto> {
    let database = require_db(&db)?;
    let id = parse_account_id(&account_id)?;
    let account =
        reconnect_pat_account(&database.pool, &auth.store, &auth.github_api_base, id).await?;
    Ok(AccountDto::from(&account))
}

#[tauri::command]
pub async fn validate_pat_account_command(
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    account_id: String,
) -> CommandResult<ProviderAccountDto> {
    let database = require_db(&db)?;
    let id = parse_account_id(&account_id)?;
    let info = validate_pat_account(&database.pool, &auth.store, &auth.github_api_base, id).await?;
    Ok(ProviderAccountDto::from(info))
}

#[tauri::command]
pub async fn get_pat_expiry_status_command(
    db: State<'_, Option<Database>>,
    account_id: String,
) -> CommandResult<PatExpiryDto> {
    let database = require_db(&db)?;
    let id = parse_account_id(&account_id)?;
    let account = get_account(&database.pool, id)
        .await?
        .ok_or_else(|| invalid_input("account not found"))?;
    let (status, days_left) = match pat_expiry_status(&account, Utc::now()) {
        PatExpiryStatus::Unknown => ("unknown".to_string(), None),
        PatExpiryStatus::Valid => ("valid".to_string(), None),
        PatExpiryStatus::ExpiringSoon { days_left } => {
            ("expiring_soon".to_string(), Some(days_left))
        }
        PatExpiryStatus::Expired => ("expired".to_string(), None),
    };
    Ok(PatExpiryDto { status, days_left })
}

// -- Auth Broker commands ---------------------------------------------

#[tauri::command]
pub async fn get_broker_status_command(
    app: AppHandle,
    auth: State<'_, AuthState>,
) -> CommandResult<BrokerStatusDto> {
    // The broker is part of the system: start it on demand before
    // reporting status, so the page sees it running instead of
    // flashing "not reachable" on a cold start. Best-effort — a
    // failure still reports reachable:false below.
    crate::broker::ensure_broker_best_effort(&app).await;
    // "Configured" now means the broker can be started: a `.env`
    // file covers everything, or the App id + key path are saved.
    // The setup token is never part of it — it resolves itself
    // (.env file, else the OS store, else generated per install),
    // so there is no token state left to report.
    let env_file_configured = crate::broker::broker_env_file_present();
    let (app_id_configured, private_key_configured) =
        match app.state::<Option<reponyx_database::Database>>().as_ref() {
            Some(database) => (
                matches!(
                    get_setting(&database.pool, "broker.app_id").await,
                    Ok(Some(setting)) if !setting.value.trim().is_empty()
                ),
                matches!(
                    get_setting(&database.pool, "broker.private_key_path").await,
                    Ok(Some(setting)) if !setting.value.trim().is_empty()
                ),
            ),
            None => (false, false),
        };
    let configured = env_file_configured || (app_id_configured && private_key_configured);
    let reachable = broker_client(&auth)?.health().await.unwrap_or(false);
    Ok(BrokerStatusDto {
        configured,
        reachable,
        base_url: auth.broker_base_url.clone(),
        app_id_configured,
        private_key_configured,
        env_file_configured,
    })
}

/// A random 32-byte setup token, hex-encoded (64 characters, well
/// over the broker's 16-character minimum). Same strength as the
/// broker's own device tokens, minus the base64 alphabet — hex
/// survives any copy-paste without confusion.
fn generate_setup_token() -> String {
    use rand::RngExt;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Open the native file picker for the GitHub App private-key file
/// (`.pem`). Returns the chosen path, or `None` when the user
/// cancels. The file's contents are never read — the path alone is
/// stored as a setting, and the broker reads the key when it
/// starts. Runs on a blocking thread: the dialog stays open as
/// long as the user takes, and must not hold an async worker.
#[tauri::command]
pub async fn pick_broker_key_file_command(app: AppHandle) -> CommandResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let handle = app.clone();
    let picked = tokio::task::spawn_blocking(move || {
        handle
            .dialog()
            .file()
            .add_filter("PEM private key", &["pem"])
            .set_title("Choose the GitHub App private-key file")
            .blocking_pick_file()
    })
    .await
    .map_err(|e| invalid_input(format!("key file picker failed: {e}")))?;
    Ok(picked
        .and_then(|path| path.into_path().ok())
        .map(|path| path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn list_broker_installations_command(
    app: AppHandle,
    auth: State<'_, AuthState>,
) -> CommandResult<Vec<BrokerInstallationDto>> {
    crate::broker::ensure_broker_best_effort(&app).await;
    let token = setup_token(&auth)?;
    let installations = broker_client(&auth)?
        .list_installations(token.expose())
        .await?;
    Ok(installations
        .into_iter()
        .map(|installation| BrokerInstallationDto {
            installation_id: installation.installation_id,
            setup_action: installation.setup_action,
            account_login: installation.account_login,
            installed_at: installation.installed_at.to_rfc3339(),
            paired_device: installation.paired_device,
        })
        .collect())
}

#[tauri::command]
pub async fn create_broker_setup_state_command(
    app: AppHandle,
    auth: State<'_, AuthState>,
) -> CommandResult<BrokerSetupStateDto> {
    crate::broker::ensure_broker_best_effort(&app).await;
    let token = setup_token(&auth)?;
    let (_state, install_url, expires_at) = broker_client(&auth)?
        .request_setup_state(token.expose())
        .await?;
    // The install URL is produced by our own broker (never free user
    // input) and is checked to be a github.com apps URL before
    // opening, so a misconfigured broker base URL cannot steer the
    // desktop at an arbitrary address.
    if !install_url.starts_with("https://github.com/apps/") {
        return Err(invalid_input(
            "broker returned an unexpected install URL; not opening it",
        ));
    }
    crate::repositories::open_with_os_app(&install_url)?;
    Ok(BrokerSetupStateDto {
        install_url,
        expires_at: expires_at.to_rfc3339(),
    })
}

/// A revoked device can never be re-paired in place — revoked is
/// terminal by design, so "Pair this device" used to fail with
/// "device pairing status cannot move from revoked to pending".
/// Register it again instead: a fresh device identifier, so the
/// old identity stays revoked and the new one pairs from scratch.
/// The broker request and the local row below both use the fresh
/// id; confirming the pairing stores it as `device.identifier`
/// for the agent, as usual.
async fn fresh_device_identifier_if_revoked(
    database: &Database,
    device_id: &str,
) -> CommandResult<String> {
    let Some(device) = get_device_by_identifier(&database.pool, device_id).await? else {
        return Ok(device_id.to_string());
    };
    if device.pairing_status != DevicePairingStatus::Revoked {
        return Ok(device_id.to_string());
    }
    let new_id = Uuid::new_v4().to_string();
    set_setting(&database.pool, "device_identifier", &new_id).await?;
    set_setting(&database.pool, "device.identifier", &new_id).await?;
    Ok(new_id)
}

/// Request a pairing code for an installation. Requires the
/// broker setup token. Returns the code (show it to the user)
/// and its expiry.
#[tauri::command]
pub async fn start_pairing_command(
    app: AppHandle,
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    installation_id: String,
) -> CommandResult<PairingDto> {
    crate::broker::ensure_broker_best_effort(&app).await;
    let database = require_db(&db)?;
    let token = setup_token(&auth)?;
    let device_id = device_identifier(database).await?;
    let device_id = fresh_device_identifier_if_revoked(database, &device_id).await?;
    let (code, expires_at) = broker_client(&auth)?
        .request_pairing_code(token.expose(), &installation_id, &device_name(), &device_id)
        .await?;
    // Record this device locally as pending (create on first use).
    match get_device_by_identifier(&database.pool, &device_id).await? {
        Some(device) => {
            if device.pairing_status != DevicePairingStatus::Pending {
                set_pairing_status(&database.pool, device.id, DevicePairingStatus::Pending).await?;
            }
        }
        None => {
            create_device(
                &database.pool,
                NewDevice {
                    device_name: device_name(),
                    device_identifier: device_id.clone(),
                },
            )
            .await?;
        }
    }
    Ok(PairingDto {
        pairing_code: code,
        expires_at: expires_at.map(|at| at.to_rfc3339()),
        installation_id,
        device_identifier: device_id,
    })
}

#[tauri::command]
pub async fn complete_pairing_command(
    app: AppHandle,
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    pairing_code: String,
) -> CommandResult<AccountDto> {
    crate::broker::ensure_broker_best_effort(&app).await;
    let database = require_db(&db)?;
    let token = setup_token(&auth)?;
    let device_id = device_identifier(database).await?;
    let (device_token, installation_id) = broker_client(&auth)?
        .confirm_pairing(token.expose(), &pairing_code, &device_id)
        .await?;
    // The device token lives in the OS credential store only.
    auth.store.store(
        SERVICE_BROKER,
        &reponyx_credential_store::device_token_account(&device_id),
        &device_token,
    )?;
    if let Some(device) = get_device_by_identifier(&database.pool, &device_id).await? {
        if device.pairing_status != DevicePairingStatus::Paired {
            set_pairing_status(&database.pool, device.id, DevicePairingStatus::Paired).await?;
        }
    }
    // Persist the device identifier under the key the agent's
    // credential resolver reads (`device.identifier`). Without this,
    // GitHub App accounts fail in the agent with "requires a paired
    // device identifier" even though pairing succeeded — the resolver
    // never sees the id generated here under `device_identifier`.
    set_setting(&database.pool, "device.identifier", &device_id).await?;
    // Validate against GitHub with the freshly minted token and
    // create (or reconnect) the account row. An installation token
    // cannot call `GET /user` (there is no user behind it — GitHub
    // answers 403 whatever the App holds), so validation reads
    // the installation's account from its first accessible
    // repository instead.
    let source = BrokerTokenSource::new(&auth.broker_base_url, &installation_id, &device_token)?;
    let provider = GitHubProvider::new(&auth.github_api_base, Arc::new(source))?;
    let info = provider.validate_installation(&installation_id).await?;
    let account =
        match get_account_by_provider(&database.pool, Provider::GitHub, &info.provider_account_id)
            .await?
        {
            Some(existing) => match existing.status {
                AccountStatus::Connected | AccountStatus::Paused => existing,
                AccountStatus::TokenExpired | AccountStatus::Revoked | AccountStatus::Error => {
                    reconnect_account(&database.pool, existing.id).await?
                }
            },
            None => {
                create_account(
                    &database.pool,
                    NewAccount {
                        provider: Provider::GitHub,
                        provider_account_id: info.provider_account_id,
                        username: info.username,
                        display_name: info.display_name,
                        label: None,
                        // `validate_installation` does not report whether the
                        // installation is for a user or an organization;
                        // default to personal (editable later, Phase 8).
                        account_type: AccountType::Personal,
                        authentication_method: AuthenticationMethod::GitHubApp,
                    },
                )
                .await?
            }
        };
    // Mirror the installation locally. The broker does not
    // report whether the installation is for a user or an
    // organization, so the type defaults to user here.
    upsert_installation(
        &database.pool,
        NewInstallation {
            account_id: account.id,
            installation_id,
            installation_type: InstallationType::User,
            permissions_json: "{}".to_string(),
        },
        InstallationStatus::Active,
    )
    .await?;
    Ok(AccountDto::from(&account))
}

#[tauri::command]
pub async fn validate_github_app_account_command(
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    account_id: String,
) -> CommandResult<ProviderAccountDto> {
    let database = require_db(&db)?;
    let id = parse_account_id(&account_id)?;
    let account = get_account(&database.pool, id)
        .await?
        .ok_or_else(|| invalid_input("account not found"))?;
    let device_id = device_identifier(database).await?;
    let device_token = auth
        .store
        .get(
            SERVICE_BROKER,
            &reponyx_credential_store::device_token_account(&device_id),
        )?
        .ok_or_else(|| CommandError {
            code: "AuthenticationExpired".to_string(),
            message: "device is not paired; pair this device with the Auth Broker first"
                .to_string(),
        })?;
    let installations = list_installations_for_account(&database.pool, account.id).await?;
    let installation = installations
        .into_iter()
        .find(|installation| installation.installation_status == InstallationStatus::Active)
        .ok_or_else(|| invalid_input("no active installation for this account"))?;
    let source = BrokerTokenSource::new(
        &auth.broker_base_url,
        &installation.installation_id,
        device_token.expose(),
    )?;
    let provider = GitHubProvider::new(&auth.github_api_base, Arc::new(source))?;
    // An installation token cannot call `GET /user` — see
    // `complete_pairing_command` above.
    let info = provider
        .validate_installation(&installation.installation_id)
        .await?;
    mark_validated(&database.pool, account.id).await?;
    Ok(ProviderAccountDto::from(info))
}

#[tauri::command]
pub async fn revoke_device_pairing_command(
    db: State<'_, Option<Database>>,
    auth: State<'_, AuthState>,
    device_identifier: String,
) -> CommandResult<bool> {
    let database = require_db(&db)?;
    let token = setup_token(&auth)?;
    let revoked = broker_client(&auth)?
        .revoke_device(token.expose(), &device_identifier)
        .await?;
    let _ = auth.store.delete(
        SERVICE_BROKER,
        &reponyx_credential_store::device_token_account(&device_identifier),
    );
    if let Some(device) = get_device_by_identifier(&database.pool, &device_identifier).await? {
        if device.pairing_status != DevicePairingStatus::Revoked {
            set_pairing_status(&database.pool, device.id, DevicePairingStatus::Revoked).await?;
        }
    }
    Ok(revoked)
}
