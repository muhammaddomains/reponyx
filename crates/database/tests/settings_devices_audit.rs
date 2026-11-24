//! Settings, devices and audit-log repository integration tests
//! (temp SQLite).

#[path = "common.rs"]
mod common;

use chrono::{Duration, Utc};
use common::{open_test_db, pool};
use reponyx_core::{DevicePairingStatus, ErrorCode};
use reponyx_database::audit::{
    AuditFilter, NewAuditLog, append_audit_log, get_audit_log, list_audit_logs,
};
use reponyx_database::devices::{
    NewDevice, create_device, delete_device, get_device, get_device_by_identifier, list_devices,
    set_pairing_status, touch_device,
};
use reponyx_database::settings::{delete_setting, get_setting, list_settings, set_setting};

#[tokio::test]
async fn settings_defaults_are_seeded_by_migration() {
    let (_dir, db) = open_test_db().await;
    let settings = list_settings(pool(&db)).await.unwrap();
    let keys: Vec<&str> = settings.iter().map(|s| s.key.as_str()).collect();
    for expected in [
        "storage.root",
        "sync.default_mode",
        "sync.concurrency",
        "ui.theme",
    ] {
        assert!(
            keys.contains(&expected),
            "missing seeded setting {expected}"
        );
    }
    let mode = get_setting(pool(&db), "sync.default_mode")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mode.value, "mirror");
    assert_eq!(
        mode.id, "setting-sync-mode",
        "seeded ids are stable strings"
    );
}

#[tokio::test]
async fn settings_set_get_update_delete() {
    let (_dir, db) = open_test_db().await;
    let created = set_setting(pool(&db), "custom.key", "one").await.unwrap();
    assert_eq!(created.value, "one");
    let fetched = get_setting(pool(&db), "custom.key").await.unwrap().unwrap();
    assert_eq!(fetched.id, created.id);
    let updated = set_setting(pool(&db), "custom.key", "two").await.unwrap();
    assert_eq!(updated.id, created.id, "update keeps the same row");
    assert_eq!(updated.value, "two");
    assert!(delete_setting(pool(&db), "custom.key").await.unwrap());
    assert!(
        get_setting(pool(&db), "custom.key")
            .await
            .unwrap()
            .is_none()
    );
    assert!(!delete_setting(pool(&db), "custom.key").await.unwrap());
}

#[tokio::test]
async fn devices_crud_lookup_and_touch() {
    let (_dir, db) = open_test_db().await;
    let created = create_device(
        pool(&db),
        NewDevice {
            device_name: "workstation".to_string(),
            device_identifier: "machine-123".to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(created.pairing_status, DevicePairingStatus::Pending);
    assert!(created.last_seen_at.is_none());
    let by_id = get_device(pool(&db), created.id).await.unwrap().unwrap();
    assert_eq!(by_id.device_name, "workstation");
    let by_identifier = get_device_by_identifier(pool(&db), "machine-123")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_identifier.id, created.id);
    assert!(
        get_device_by_identifier(pool(&db), "nope")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(list_devices(pool(&db)).await.unwrap().len(), 1);
    let touched = touch_device(pool(&db), created.id).await.unwrap();
    assert!(touched.last_seen_at.is_some());
    assert!(delete_device(pool(&db), created.id).await.unwrap());
}

#[tokio::test]
async fn devices_pairing_transitions() {
    let (_dir, db) = open_test_db().await;
    let created = create_device(
        pool(&db),
        NewDevice {
            device_name: "laptop".to_string(),
            device_identifier: "machine-456".to_string(),
        },
    )
    .await
    .unwrap();
    let paired = set_pairing_status(pool(&db), created.id, DevicePairingStatus::Paired)
        .await
        .unwrap();
    assert_eq!(paired.pairing_status, DevicePairingStatus::Paired);
    // A paired device may request a fresh pairing code (re-pair).
    let pending_again = set_pairing_status(pool(&db), created.id, DevicePairingStatus::Pending)
        .await
        .unwrap();
    assert_eq!(pending_again.pairing_status, DevicePairingStatus::Pending);
    let paired_again = set_pairing_status(pool(&db), created.id, DevicePairingStatus::Paired)
        .await
        .unwrap();
    assert_eq!(paired_again.pairing_status, DevicePairingStatus::Paired);
    let revoked = set_pairing_status(pool(&db), created.id, DevicePairingStatus::Revoked)
        .await
        .unwrap();
    assert_eq!(revoked.pairing_status, DevicePairingStatus::Revoked);
    assert_eq!(
        set_pairing_status(pool(&db), created.id, DevicePairingStatus::Pending)
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput,
        "revoked is terminal"
    );
    assert_eq!(
        set_pairing_status(pool(&db), created.id, DevicePairingStatus::Paired)
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput,
        "revoked is terminal"
    );
}

#[tokio::test]
async fn devices_duplicate_identifier_rejected() {
    let (_dir, db) = open_test_db().await;
    let input = NewDevice {
        device_name: "one".to_string(),
        device_identifier: "dup".to_string(),
    };
    create_device(pool(&db), input.clone()).await.unwrap();
    assert_eq!(
        create_device(pool(&db), input).await.unwrap_err().code,
        ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn audit_append_and_list_with_filters() {
    let (_dir, db) = open_test_db().await;
    let first = append_audit_log(
        pool(&db),
        NewAuditLog {
            actor: "user".to_string(),
            action: "account.connected".to_string(),
            resource_type: "account".to_string(),
            resource_id: Some("acc-1".to_string()),
            metadata: Some(serde_json::json!({"provider": "github"})),
        },
    )
    .await
    .unwrap();
    assert_eq!(first.metadata, r#"{"provider":"github"}"#);
    let second = append_audit_log(
        pool(&db),
        NewAuditLog {
            actor: "agent".to_string(),
            action: "sync.completed".to_string(),
            resource_type: "repository".to_string(),
            resource_id: Some("repo-1".to_string()),
            metadata: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(second.metadata, "{}");

    let fetched = get_audit_log(pool(&db), first.id).await.unwrap().unwrap();
    assert_eq!(fetched.action, "account.connected");

    let by_actor = list_audit_logs(
        pool(&db),
        AuditFilter {
            actor: Some("user".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(by_actor.len(), 1);
    assert_eq!(by_actor[0].id, first.id);

    let by_resource = list_audit_logs(
        pool(&db),
        AuditFilter {
            resource_type: Some("repository".to_string()),
            resource_id: Some("repo-1".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(by_resource.len(), 1);
    assert_eq!(by_resource[0].id, second.id);

    // Time range: everything is within the last hour; nothing is
    // before an hour ago.
    let recent = list_audit_logs(
        pool(&db),
        AuditFilter {
            from: Some(Utc::now() - Duration::hours(1)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(recent.len(), 2);
    let old = list_audit_logs(
        pool(&db),
        AuditFilter {
            to: Some(Utc::now() - Duration::hours(1)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(old.is_empty());

    // Newest first.
    let all = list_audit_logs(pool(&db), AuditFilter::default())
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, second.id);
}

#[tokio::test]
async fn audit_rejects_bad_input() {
    let (_dir, db) = open_test_db().await;
    let err = append_audit_log(
        pool(&db),
        NewAuditLog {
            actor: "  ".to_string(),
            action: "x".to_string(),
            resource_type: "y".to_string(),
            resource_id: None,
            metadata: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    let err = append_audit_log(
        pool(&db),
        NewAuditLog {
            actor: "user".to_string(),
            action: "x".to_string(),
            resource_type: "y".to_string(),
            resource_id: None,
            metadata: Some(serde_json::json!("not an object")),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}
