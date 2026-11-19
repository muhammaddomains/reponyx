//! GitHub API response types and their mapping to RepoNyx domain
//! types.
//!
//! The `Api*` structs are the wire shapes (serde, `deny_unknown_fields`
//! is deliberately NOT set — GitHub adds fields over time and the client
//! must not break when it does). Mapping functions are pure and unit
//! tested; they never identify a repository by name alone.

use chrono::{DateTime, Utc};
use reponyx_core::{ErrorCode, InstallationType, RepoNyxError, Visibility};
use reponyx_provider_abstractions::{DiscoveredRepository, ProviderAccountInfo};
use serde::Deserialize;

/// The `owner` object embedded in repository responses.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiOwner {
    pub login: String,
    #[serde(rename = "type")]
    pub account_type: Option<String>,
}

/// One repository as returned by `GET /user/repos` (and the
/// installation-repositories endpoints). Only the fields RepoNyx uses
/// are modelled.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiRepository {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: ApiOwner,
    pub private: bool,
    /// `"public" | "private" | "internal"`; absent on older responses.
    pub visibility: Option<String>,
    pub default_branch: Option<String>,
    pub clone_url: String,
    pub size: Option<i64>,
    pub updated_at: Option<DateTime<Utc>>,
    pub archived: bool,
    pub fork: bool,
    /// Present on single-repo responses (`GET /repos/{owner}/{repo}`).
    pub permissions: Option<ApiPermissions>,
}

/// The `permissions` object on a repository: what the token may do.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiPermissions {
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub maintain: bool,
    #[serde(default)]
    pub push: bool,
    #[serde(default)]
    pub triage: bool,
    #[serde(default)]
    pub pull: bool,
}

/// The authenticated user (`GET /user`).
#[derive(Debug, Clone, Deserialize)]
pub struct ApiUser {
    pub id: u64,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    #[serde(rename = "type")]
    pub account_type: Option<String>,
}

/// The `account` object embedded in installation responses.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiInstallationAccount {
    pub login: String,
    #[serde(rename = "type")]
    pub account_type: Option<String>,
}

/// One installation as returned by `GET /user/installations`.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiInstallation {
    pub id: u64,
    pub account: ApiInstallationAccount,
    /// Permission map, e.g. `{"contents": "read", "metadata": "read"}`.
    pub permissions: serde_json::Value,
    /// `"all"` or `"selected"`.
    pub repository_selection: Option<String>,
    /// Non-null when the installation is suspended.
    pub suspended_at: Option<DateTime<Utc>>,
    /// GitHub App id / slug. Not stored in the V1 schema (see
    /// `GitHubInstallation` docs) — kept here so callers can see them.
    pub app_id: Option<i64>,
    pub app_slug: Option<String>,
}

/// A GitHub installation, mapped from the API into RepoNyx terms.
///
/// The V1 `installations` table stores `installation_id`,
/// `account_id`, `installation_type`, `installation_status` and
/// `permissions_json`. These API fields have **no column** and are
/// therefore not persisted (recorded here so the gap is explicit):
/// `app_id`, `app_slug`, `repository_selection`, and the
/// `suspended_at` timestamp (only its presence — suspended or not —
/// is stored, as the status). If a later phase needs them, add a V3
/// migration; do not widen this struct silently.
#[derive(Debug, Clone)]
pub struct GitHubInstallation {
    /// GitHub installation id, as a string (the DB column is TEXT).
    pub installation_id: String,
    pub account_login: String,
    pub installation_type: InstallationType,
    /// `true` when `suspended_at` was non-null.
    pub suspended: bool,
    /// The raw permissions object, serialised to `permissions_json`.
    pub permissions: serde_json::Value,
    pub repository_selection: Option<String>,
    pub app_id: Option<i64>,
    pub app_slug: Option<String>,
}

/// Permission level of the token on one repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionLevel {
    Admin,
    Write,
    Read,
    None,
}

/// What the token may do on one repository (`GET /repos/{o}/{r}`).
#[derive(Debug, Clone)]
pub struct RepositoryAccess {
    pub full_name: String,
    pub permission: PermissionLevel,
    /// `true` when the token may read contents (`pull` or better).
    pub can_read_contents: bool,
    pub default_branch: Option<String>,
    pub private: bool,
}

/// Map the API visibility fields to RepoNyx `Visibility`.
///
/// GitHub sends both `private: bool` and `visibility: "public" |
/// "private" | "internal"`. `visibility` wins when present; otherwise
/// `private` decides. An unknown `visibility` string is an error,
/// never silently defaulted.
pub fn map_visibility(private: bool, visibility: Option<&str>) -> Result<Visibility, RepoNyxError> {
    match visibility {
        Some("public") => Ok(Visibility::Public),
        Some("private") => Ok(Visibility::Private),
        Some("internal") => Ok(Visibility::Internal),
        Some(other) => Err(RepoNyxError::new(
            ErrorCode::InvalidInput,
            format!("unknown repository visibility: {other}"),
        )),
        None => Ok(if private {
            Visibility::Private
        } else {
            Visibility::Public
        }),
    }
}

/// Strip embedded credentials from a clone URL, defensively.
///
/// GitHub's `clone_url` never contains credentials, but a compromised
/// or misbehaving server (or a future API change) must not be able to
/// smuggle `https://user:token@host/...` into the database. If a
/// userinfo part is present it is removed; the rest of the URL is
/// kept verbatim. URLs without a scheme are returned unchanged.
pub fn strip_embedded_credentials(url: &str) -> String {
    let Some(after_scheme) = url.split("://").nth(1) else {
        return url.to_string();
    };
    let Some(at) = after_scheme
        .split('/')
        .next()
        .and_then(|auth| auth.rfind('@'))
    else {
        return url.to_string();
    };
    let scheme = url.split("://").next().unwrap_or("https");
    let host_and_path = &after_scheme[at + 1..];
    format!("{scheme}://{host_and_path}")
}

/// Map one API repository to a `DiscoveredRepository`. The provider id
/// is GitHub's numeric `id` as a string — never the name.
pub fn map_repository(api: ApiRepository) -> Result<DiscoveredRepository, RepoNyxError> {
    Ok(DiscoveredRepository {
        provider_repository_id: api.id.to_string(),
        owner: api.owner.login,
        name: api.name,
        full_name: api.full_name,
        visibility: map_visibility(api.private, api.visibility.as_deref())?,
        default_branch: api.default_branch,
        clone_url: strip_embedded_credentials(&api.clone_url),
        archived: api.archived,
        fork: api.fork,
        size_kb: api.size,
        remote_updated_at: api.updated_at,
    })
}

pub fn map_account(api: ApiUser) -> ProviderAccountInfo {
    ProviderAccountInfo {
        provider_account_id: api.id.to_string(),
        username: api.login,
        display_name: api.name,
        avatar_url: api.avatar_url,
    }
}

pub fn map_installation(api: ApiInstallation) -> GitHubInstallation {
    let installation_type = match api.account.account_type.as_deref() {
        Some("Organization") => InstallationType::Organization,
        _ => InstallationType::User,
    };
    GitHubInstallation {
        installation_id: api.id.to_string(),
        account_login: api.account.login,
        installation_type,
        suspended: api.suspended_at.is_some(),
        permissions: api.permissions,
        repository_selection: api.repository_selection,
        app_id: api.app_id,
        app_slug: api.app_slug,
    }
}

pub fn map_permissions(api: ApiPermissions) -> PermissionLevel {
    if api.admin {
        PermissionLevel::Admin
    } else if api.push || api.maintain {
        PermissionLevel::Write
    } else if api.pull || api.triage {
        PermissionLevel::Read
    } else {
        PermissionLevel::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_repo() -> ApiRepository {
        ApiRepository {
            id: 123,
            name: "reponyx".to_string(),
            full_name: "octocat/reponyx".to_string(),
            owner: ApiOwner {
                login: "octocat".to_string(),
                account_type: Some("User".to_string()),
            },
            private: false,
            visibility: Some("public".to_string()),
            default_branch: Some("main".to_string()),
            clone_url: "https://github.com/octocat/reponyx.git".to_string(),
            size: Some(42),
            updated_at: None,
            archived: false,
            fork: false,
            permissions: None,
        }
    }

    #[test]
    fn visibility_prefers_visibility_field() {
        assert_eq!(
            map_visibility(false, Some("internal")).unwrap(),
            Visibility::Internal
        );
        assert_eq!(
            map_visibility(true, Some("public")).unwrap(),
            Visibility::Public
        );
    }

    #[test]
    fn visibility_falls_back_to_private_flag() {
        assert_eq!(map_visibility(true, None).unwrap(), Visibility::Private);
        assert_eq!(map_visibility(false, None).unwrap(), Visibility::Public);
    }

    #[test]
    fn visibility_rejects_unknown_values() {
        assert!(map_visibility(false, Some("secret")).is_err());
    }

    #[test]
    fn clone_url_credentials_are_stripped() {
        assert_eq!(
            strip_embedded_credentials("https://user:token-value@github.com/o/r.git"),
            "https://github.com/o/r.git"
        );
        assert_eq!(
            strip_embedded_credentials("https://github.com/o/r.git"),
            "https://github.com/o/r.git"
        );
        assert_eq!(
            strip_embedded_credentials("git@github.com:o/r.git"),
            "git@github.com:o/r.git"
        );
    }

    #[test]
    fn repository_mapping_uses_numeric_id() {
        let mapped = map_repository(api_repo()).unwrap();
        assert_eq!(mapped.provider_repository_id, "123");
        assert_eq!(mapped.full_name, "octocat/reponyx");
        assert_eq!(mapped.visibility, Visibility::Public);
        assert_eq!(mapped.size_kb, Some(42));
        assert!(!mapped.archived);
        assert!(!mapped.fork);
    }

    #[test]
    fn repository_mapping_strips_credentials_from_clone_url() {
        let mut api = api_repo();
        api.clone_url = "https://user:token-value@github.com/octocat/reponyx.git".to_string();
        let mapped = map_repository(api).unwrap();
        assert_eq!(mapped.clone_url, "https://github.com/octocat/reponyx.git");
        assert!(!mapped.clone_url.contains("token-value"));
    }

    #[test]
    fn permissions_map_to_levels() {
        assert_eq!(
            map_permissions(ApiPermissions {
                admin: true,
                maintain: false,
                push: false,
                triage: false,
                pull: false,
            }),
            PermissionLevel::Admin
        );
        assert_eq!(
            map_permissions(ApiPermissions {
                admin: false,
                maintain: false,
                push: true,
                triage: false,
                pull: true,
            }),
            PermissionLevel::Write
        );
        assert_eq!(
            map_permissions(ApiPermissions {
                admin: false,
                maintain: false,
                push: false,
                triage: false,
                pull: true,
            }),
            PermissionLevel::Read
        );
        assert_eq!(
            map_permissions(ApiPermissions {
                admin: false,
                maintain: false,
                push: false,
                triage: false,
                pull: false,
            }),
            PermissionLevel::None
        );
    }

    #[test]
    fn installation_mapping_marks_suspended() {
        let api = ApiInstallation {
            id: 7,
            account: ApiInstallationAccount {
                login: "acme".to_string(),
                account_type: Some("Organization".to_string()),
            },
            permissions: serde_json::json!({"contents": "read"}),
            repository_selection: Some("selected".to_string()),
            suspended_at: Some(Utc::now()),
            app_id: Some(99),
            app_slug: Some("reponyx".to_string()),
        };
        let mapped = map_installation(api);
        assert_eq!(mapped.installation_id, "7");
        assert_eq!(mapped.installation_type, InstallationType::Organization);
        assert!(mapped.suspended);
        assert_eq!(mapped.app_slug.as_deref(), Some("reponyx"));
    }
}
