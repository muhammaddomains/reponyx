//! Credential-store integration tests: the OS backend probes
//! availability and skips gracefully when no Secret Service /
//! Keychain answers (this Linux VM has none); the in-memory backend
//! is exercised fully. No real credentials anywhere.

use reponyx_credential_store::{CredentialStore, OsCredentialStore, SERVICE_PAT, memory_store};

#[test]
fn memory_store_round_trip_via_trait_object() {
    let store = memory_store();
    assert_eq!(store.get(SERVICE_PAT, "acc-1").unwrap(), None);
    store
        .store(SERVICE_PAT, "acc-1", "test-pat-secret")
        .unwrap();
    assert!(store.exists(SERVICE_PAT, "acc-1").unwrap());
    assert_eq!(
        store.get(SERVICE_PAT, "acc-1").unwrap().unwrap().expose(),
        "test-pat-secret"
    );
    store.delete(SERVICE_PAT, "acc-1").unwrap();
    assert_eq!(store.get(SERVICE_PAT, "acc-1").unwrap(), None);
    assert_eq!(store.list_accounts(SERVICE_PAT).unwrap().len(), 0);
}

#[test]
fn os_store_skips_gracefully_when_unavailable() {
    if !OsCredentialStore::is_available() {
        eprintln!("SKIP: no OS credential store available on this machine");
        return;
    }
    // The store answered the probe: a full round trip must work.
    let store = OsCredentialStore::new();
    let account = "reponyx-phase5-probe";
    let _ = store.delete(SERVICE_PAT, account);
    store
        .store(SERVICE_PAT, account, "test-pat-probe-value")
        .unwrap();
    assert_eq!(
        store.get(SERVICE_PAT, account).unwrap().unwrap().expose(),
        "test-pat-probe-value"
    );
    store.delete(SERVICE_PAT, account).unwrap();
    assert_eq!(store.get(SERVICE_PAT, account).unwrap(), None);
}

#[test]
fn os_store_listing_is_unsupported_not_silent() {
    // Listing must fail loudly, never pretend the store is empty.
    let store = OsCredentialStore::new();
    let err = store.list_accounts(SERVICE_PAT).unwrap_err();
    assert_eq!(err.code, reponyx_core::ErrorCode::CredentialStoreError);
}
