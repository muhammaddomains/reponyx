//! Integration tests: branch/tag discovery from a mirror.

#[path = "common.rs"]
mod common;

use common::seeded_bare_repo;
use reponyx_git_engine::locks::RepoLockManager;
use reponyx_git_engine::mirror::mirror_clone;
use reponyx_git_engine::options::SyncOptions;
use reponyx_git_engine::refs::{RefKind, head_sha, list_branches, list_tags};
use std::path::Path;

#[tokio::test]
async fn branches_and_tags_from_mirror() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = seeded_bare_repo(dir.path()).await;
    let locks = RepoLockManager::new();
    let mirror = dir.path().join("mirror.git");
    let url = fixture.bare.to_string_lossy().into_owned();
    mirror_clone(
        Path::new("git"),
        &url,
        &mirror,
        &locks,
        SyncOptions::default(),
    )
    .await
    .unwrap();

    let branches = list_branches(Path::new("git"), &mirror).await.unwrap();
    assert_eq!(branches.len(), 3);
    let main = branches.iter().find(|b| b.name == "main").unwrap();
    assert_eq!(main.sha, fixture.head_sha);
    assert_eq!(main.full_ref, "refs/heads/main");
    assert_eq!(main.kind, RefKind::LocalBranch);
    assert_eq!(main.remote, None);
    let feature = branches.iter().find(|b| b.name == "feature").unwrap();
    assert_eq!(feature.sha, fixture.feature_sha);

    let tags = list_tags(Path::new("git"), &mirror).await.unwrap();
    assert_eq!(tags.len(), 2);
    let v1 = tags.iter().find(|t| t.name == "v1.0.0").unwrap();
    assert!(v1.annotated);
    assert_eq!(v1.sha, fixture.annotated_tag_sha);
    assert_eq!(v1.tag_sha, fixture.annotated_tag_object);
    assert_ne!(v1.sha, v1.tag_sha);
    let v09 = tags.iter().find(|t| t.name == "v0.9").unwrap();
    assert!(!v09.annotated);
    assert_eq!(v09.sha, fixture.light_tag_sha);
    assert_eq!(v09.tag_sha, fixture.light_tag_sha);

    assert_eq!(
        head_sha(Path::new("git"), &mirror)
            .await
            .unwrap()
            .as_deref(),
        Some(fixture.head_sha.as_str())
    );
}
