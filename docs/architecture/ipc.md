# IPC Protocol

Status: Accepted (Phase 1) · Code landed 2026-11-21, NOT verified
complete — see `ai/memory/PROJECT_STATUS.md` ("Phase 6 / Phase 7
verification"). The protocol v1 types, frame codec, shared-secret
auth, server, and client exist in `crates/ipc`, but the crate is not
a workspace member (cannot be built), progress push is unwired
(`set_progress_source` has no callers), the client has no
`cancel_job` method, and there are no server/client/auth tests.

## Transport

- Windows: named pipes. macOS/Linux: Unix domain sockets.
- No TCP/network listener. The pipe/socket is created with OS-level
  ACLs so only the owning user (and the agent) can connect.

## Protocol (planned, versioned)

- Every message carries a protocol version (`1` initially) and a
  message type; unknown versions are rejected, not guessed at.
- Message types (planned): `ping`/`pong` (health), `enqueue` (sync
  now, by repository/account), `cancel_job`, `pause`/`resume`,
  `progress` (job id, phase, bytes), `job_finished`,
  `agent_status`, `shutdown`.
- All messages are validated (schema + bounds) before any action.
- The desktop must authenticate to the agent (per-user secret
  established at install / first run, stored in the OS credential
  store); the agent rejects unauthenticated or malformed messages.
- Disconnections are detected; the desktop shows
  `BackgroundAgentUnavailable` and retries with backoff. Agent restart
  is detected via a generation/epoch id in `agent_status`.

## Security

- Arbitrary local processes must not be able to issue sync commands:
  ACLs + authentication + strict validation (see threat model §9).
- No secrets are sent over IPC except short-lived credentials issued
  for a specific operation, and never in cleartext outside the pipe.
- IPC traffic is not logged with payloads; only message types and
  counts are logged.
