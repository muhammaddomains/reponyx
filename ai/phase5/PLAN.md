# Phase 5 — Authentication

- Status: Complete 2026-11-18 (on `main`, commit pending — see Completion status)
- Depends on: Phase 1, Phase 4

## Objectives

Both authentication methods working end to end: GitHub App (primary)
via the Auth Broker, and fine-grained PAT (alternative) via the OS
credential store.

## Requirements

- `apps/auth-broker`: holds the GitHub App private key (only there);
  installation callback; token minting; device pairing; rate limiting;
  audit events; no secret logging.
- Secure one-time pairing: random, single-use, expiring, revocable,
  audited, device-identified.
- `crates/credential-store`: Windows Credential Manager (Keychain /
  Secret Service on other platforms); PAT validation, expiration
  detection, replacement, removal, reconnection.
- Mixed auth: App + PAT accounts coexist; sync engine is indifferent.
- Explicit approval gates: no GitHub App publishing, no production
  credentials, no broker deployment without user approval.

## Implementation tasks

- [x] Auth Broker service (callback, token minting, pairing, audit)
- [x] Credential-store crate
- [x] PAT account flow in desktop (add/validate/replace/remove)
- [x] GitHub App install + pairing flow in desktop
- [x] Token expiration warnings + reconnect UX

## Dependencies

Phase 4 provider; Phase 2 accounts/installations/devices tables.

## Acceptance criteria

- A GitHub App installation can be paired to the desktop and yields
  working discovery; a PAT account validates and discovers repos.
- Private key is provably absent from the desktop binary (CI scan).
- Expired/revoked credentials surface actionable errors, no infinite retry.

## Testing

- Mocked broker + GitHub API; pairing expiry/reuse/revocation tests;
  credential-store round-trip tests (OS store or mocked backend in CI).

## Known issues

- **Broker state is in-memory.** Restarting the broker forgets
  installations, pairing codes, and device tokens; devices must
  re-pair. Durable broker storage is a hardening item for the
  deployment phase.
- **Binary private-key scan is partial on this VM.** The scan
  (`scripts/check-no-private-key.sh`) checks desktop sources and the
  dependency tree (both pass); the binary part is SKIPPED because no
  desktop binary can be built here (missing WebViewGTK libs). Run the
  full scan on a machine where `cargo tauri build` works, before
  release (Phase 10).
- **OS credential store unavailable on this VM.** The OS-backend test
  skips gracefully (no Secret Service here); round trips are covered
  by the in-memory backend. Windows Credential Manager / macOS
  Keychain need a machine with those stores (Phase 10).
- **One transient failure during verification:** the untouched Phase 3
  test `healthy_mirror_and_working_clone` (git-engine) failed under
  full parallel load (`Locked` instead of `Healthy`) — once during
  the builder's verification, twice more during independent
  verification (2 failures in ~8 full-workspace runs; never in
  isolation, never on re-run). No Phase 5 code touches git-engine.
  Fixed after the Phase 5 commit (follow-up commit, test only):
  the test now retries `verify` up to 3 times (100 ms apart) while
  the diagnosis is transiently `Locked`, via a `verify_fresh_clone`
  helper in `crates/git-engine/tests/health.rs`; any other status,
  including a persistent `Locked`, still fails the test. No
  production code changed.
- **401 refresh changes a Phase 4 test's contract.** The client now
  refreshes once after a 401 and retries a single time, so the
  `unauthorized_*` errors test expects 2 requests instead of 1, and
  the PAT 401 test expects 2 attempts. Both are the designed
  behaviour (exactly one refresh, then fail fast) — see
  `crates/github-provider/src/client.rs`.

## Deliberately not done (needs user approval)

- No GitHub App was published or created; the broker only accepts an
  App id/slug/key supplied by the operator via environment.
- No production credential was created or modified; all tests use
  obviously fake tokens (`ghp_0000…`, `test-…`) and throwaway RSA
  keys generated in-memory.
- The Auth Broker was not deployed anywhere; it binds loopback
  (`127.0.0.1:8765`) by default.
- File-backed encrypted credential storage is out of scope (the
  in-memory backend covers tests/CI; the OS backend covers
  production).

## Completion status

Complete 2026-11-18. Verification (actually run): `cargo fmt
--check` pass; clippy (`--workspace --exclude reponyx-desktop
--all-targets`) pass, 0 warnings; `cargo test --workspace --exclude
reponyx-desktop` **262 passed, 0 failed** (auth-broker 22,
core 11, credential-store 20, database 55, git-engine 87,
github-provider 63, logging 4, provider-abstractions 0) — Phase 4
baseline was 215; `scripts/check-no-private-key.sh` pass (binary
part skipped, see Known issues); `pnpm -r typecheck` pass (Node
26.13.1); `pnpm --filter @reponyx/desktop build` pass (2306 modules,
1.95 s); desktop eslint pass. Tauri binary build still blocked on
this VM by missing `gdk-3.0` / WebViewGTK system libraries — not
claimed passed. Commit on `main`; not pushed (parent verifies and
pushes).
