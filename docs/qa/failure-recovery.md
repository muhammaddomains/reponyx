# Failure-recovery coverage

Phase 10 inventory (2026-11-22). The Phase 10 plan asks for an
integration + failure-recovery test suite covering expired/revoked
tokens, network loss, rate limits, disk full, process kill, corrupt
repos, renames, remote deletion, and restarts. The tests already
existed across the crates; this document is the map from scenario
to the test that proves it, and says plainly what is simulated
rather than real.

## Covered

| Scenario | Proof | How real |
| --- | --- | --- |
| Token expires mid-flow | `unauthorized_refreshes_once_then_fails_as_authentication_expired` (github-provider) | Wiremock 401 then 401 again: refresh happens once, then the account is `AuthenticationExpired`. |
| Token revoked | `forbidden_revoked_is_authentication_revoked` (github-provider) | Wiremock 403 with a revocation body: account marked revoked, jobs stop, archive preserved. |
| Reconnect after expiry | `reconnect_restores_expired_account_and_401_re_expires_it` (github-provider) | Reconnect issues a fresh token; a second 401 re-expires it. |
| PAT expiry | `expiry_status_is_computed_from_stored_expiry`, `set_pat_expiry` (database) | Expiry read from stored metadata; warnings surface before lapse. |
| PAT removed | `remove_pat_deletes_secret_and_revokes_account` (credential-store) | Secret deleted from the store and the account revoked in one operation. |
| Network loss | `unreachable_api_is_network_unavailable` (github-provider) | Unroutable endpoint: `NetworkUnavailable`, no retry storm. |
| Bad PAT without network | `add_rejects_bad_pat_format_without_network` (credential-store) | Format is rejected locally before any network call. |
| Rate limit (API) | `forbidden_rate_limit_is_retried_like_429`, `rate_limit_mid_pagination_backs_off_and_resumes` (github-provider) | Wiremock 429/403-rate-limit: backoff, then resume where pagination stopped. |
| Rate limit (queue) | `rate_limiter_defers_job_starts`, `wait_beyond_the_cap_returns_rate_limit_exceeded` (sync-queue) | Token buckets (global 120/min, per-account 60/min) defer starts; waiting past the cap fails cleanly. |
| Disk full | `mirror_clone_fails_when_disk_cannot_grow` (git-engine, `tests/disk_full.rs`) | Real: `RLIMIT_FSIZE` 1 MiB against a ~3 MiB pack transfer. Clone fails with a Git/DiskSpace error, and the partial destination does not verify as healthy. |
| Process kill / restart | `lease_expiry_drives_recovery`, `recovery_requeues_abandoned_running_jobs` (sync-queue) | Simulated via DB state: a running job whose lease expired is requeued. No test kills a real process (see gaps). |
| Corrupt repo | `corrupt_object_is_caught_by_fsck`, `corrupt_head_is_corrupt`, `verify_reports_missing_and_corrupt_objects` (git-engine) | Real: corrupted object files in a fixture repo; fsck/verify catch them. |
| Corrupt LFS object | LFS fetch tests (git-engine) | Real: corrupt objects are deleted first (git lfs fetch skips existing files), then re-fetched and SHA-256 verified. |
| Rename | `rename_updates_identity_and_clone_url`, `rename_collision_is_rejected_and_other_repo_untouched` (database) | Rename updates owner/name/full_name/clone_url but not local_path (stale archive dirs are handled by the storage move command). |
| Remote deletion | `discovery_upsert_preserves_local_state_and_reports_not_seen` (github-provider) | A repo absent from discovery is reported, never deleted locally. |
| Cancel mid-flight | `cancel_mirror_fetch_mid_flight`, `cancel_mirror_clone_leaves_diagnosable_destination`, `request_cancel_pending_and_running` (git-engine, sync-queue) | Real cancellation of a running git process; the partial destination stays diagnosable. |
| Concurrent writes | `concurrent_writes_all_succeed_without_corruption` (database) | Real concurrent writers against one SQLite file (WAL). |
| App upgrade (DB) | `v1_database_upgrades_to_latest_with_data_intact`, `v3_database_upgrades_to_v4_and_dedupes` (database, `tests/migrations.rs`) | Real: a V1 (resp. V3) database is built from the actual migration files, seeded with V1-shape rows, then upgraded with the real migrator; data, indexes, and app functions are verified afterwards. |

## Gaps (not covered, not claimed)

- **No real process-kill test.** Nobody runs `kill -9` on the agent
  mid-clone and watches the restart recover. The lease mechanism is
  proven at the DB layer; the end-to-end kill path is a Windows
  verification item.
- **No mid-transfer network loss.** The network tests cover a dead
  endpoint before a request starts, not a connection dropped halfway
  through a fetch. Git's own retry/failure surfaces the error; the
  queue retries it — but that exact sequence has no test.
- **No Windows failure tests.** Task Scheduler, Credential Manager,
  and the installer paths have no failure tests on this Linux VM;
  they are Phase 10 Windows-verification items.
