# Phase 4 — GitHub Integration

- Status: Complete (2026-11-18, on `main`)
- Depends on: Phase 1 (provider trait), Phase 2 (repositories)

## Objectives

Implement the GitHub provider: API client, paginated repository
discovery, account/installation metadata, permission validation.

## Requirements

- `crates/github-provider` implementing `GitProvider`.
- Correct pagination (never assume the first page is complete);
  streaming/batched processing for thousands of repos.
- Repository metadata: visibility, default branch, size, updated_at,
  archived/fork flags, clone URL (credential-free).
- Rate-limit awareness (headers, backoff) and permission validation.
- Discovery writes to the `repositories` table via Phase 2 repositories.

## Implementation tasks

- [x] GitHub API client (reqwest, auth header injection, retries)
- [x] Repository discovery with pagination + cursors
- [x] Installation metadata + repository permission checks
- [x] Account metadata (username, avatar)
- [x] Mocked-API integration tests (no real credentials)

## Dependencies

Phase 1 trait; Phase 2 DB repositories; Phase 5 for real credentials
(discovery can be tested with mocked tokens).

## Acceptance criteria

- Discovery of a multi-page mocked account yields all repos, in order,
  with correct metadata; rate-limit responses back off and resume.
- No repo is identified by name alone.

## Testing

- Mocked GitHub API (wiremock or local server) for pagination, errors,
  rate limits; unit tests for mapping.

## Known issues

- wiremock-rs has no scenarios (that is a Java WireMock feature):
  stateful multi-response sequences in tests use a
  `SequencedResponder` (`tests/common.rs`) that answers successive
  requests with queued responses, in order.

## Completion status

Complete (2026-11-18, on `main`). Verification: `cargo fmt
--check` pass; clippy (`--workspace --exclude reponyx-desktop
--all-targets`) pass, 0 warnings; `cargo test --workspace --exclude
reponyx-desktop` **215 passed, 0 failed** (core 11, database 55,
git-engine 87, github-provider 58, logging 4,
provider-abstractions 0); `pnpm -r typecheck` pass; desktop build
pass. Tauri binary build still blocked on this VM — not claimed
passed.
