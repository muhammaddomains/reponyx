# Threat Model

Status: **Reviewed 2026-11-22 (Phase 10 security audit).** Every
mitigation below was checked against the code as it stands after
Phase 9; the findings from that review are in "Phase 10 findings"
at the end. The Phase 1 draft's wording is kept where it still
holds and corrected where the code says otherwise.

Scope: RepoNyx desktop + background agent + Auth Broker + GitHub
integration. Out of scope: GitHub's own infrastructure, the user's
operating system, and the physical security of GitHub's data centers.

Security is not claimed because "encryption is used". Each threat
below names the boundary, the mitigation, and the residual risk.

## 1. Malicious local processes

- **Threat:** Another process on the machine reads the SQLite DB,
  scrapes memory, or issues IPC commands to trigger syncs/deletes.
- **Mitigation (verified 2026-11-22):** The desktop↔agent IPC is
  gated by a per-install 32-byte shared secret stored in the OS
  credential store (`crates/ipc/src/auth.rs`) — every IPC message
  carries the token, compared in constant time; no network
  listener; DB file under the user's profile with OS file
  permissions; no secrets in the DB.
- **Residual:** A process running as the same user can read the user's
  files. RepoNyx cannot defend against same-user malware; OS account
  separation is the boundary. Note: the Windows named pipe itself
  has no ACL configured (see finding F2) — the token check, not the
  pipe, is the gate.

## 2. Stolen PATs

- **Threat:** A PAT is exfiltrated from the machine or logs.
- **Mitigation:** PATs only in the OS credential store (DPAPI on
  Windows); never in SQLite, logs, URLs, or argv; fine-grained PATs
  with Contents: Read / Metadata: Read only; expiration detection.
- **Residual:** A stolen PAT still grants read access to its repos
  until revoked/expired. Users must be able to revoke and replace
  quickly (Phase 5 reconnect flow).

## 3. Leaked GitHub App private key

- **Threat:** The private key escapes the broker and is used to mint
  installation tokens for all installations.
- **Mitigation (verified 2026-11-22):** Key exists only in the
  broker environment; never in the desktop binary, repo, logs, or CI
  artifacts; secret scanning in CI (`scripts/check-no-private-key.sh`,
  run by the lint workflow, fails the build on a match); short-lived
  tokens; least-privilege permissions.
- **Residual:** Compromise of the broker host is total for installed
  accounts until the key is rotated and installations re-authorized.
  Rotation is documented in `docs/security/key-rotation-runbook.md`
  (finding F3, fixed 2026-11-23).

## 4. Unauthorized device pairing

- **Threat:** An attacker pairs their device to a victim's GitHub
  installation and receives its repositories.
- **Mitigation (verified 2026-11-22, pairing expiry removed
  2026-11-24):** Cryptographically random pairing codes, stored
  only as SHA-256 hashes, single-use (a confirmed/revoked code is
  rejected on reuse), no expiry — the code is bound to one device,
  confirm requires the setup token, and wrong-device attempts
  revoke the code after 5 tries — per-IP rate limiting, device
  identification, explicit user confirmation in the browser flow;
  revocation + audit history.
- **Residual:** If an attacker also controls the victim's GitHub
  session, pairing cannot distinguish them; GitHub session security
  is the boundary. The audit history is durable (SQLite, F1 fixed
  2026-11-22) with a 90-day retention prune at startup.

## 5. Compromised Auth Broker

- **Threat:** Attacker controls the broker: mints tokens, harvests
  installation metadata, or serves malicious pairing flows.
- **Mitigation (verified 2026-11-22):** Least-privilege app
  permissions; tokens expire in ~1 h; append-only audit log
  (`apps/auth-broker/src/audit.rs`); rate limits; TLS; broker has no
  access to local archives (it never sees repository contents).
- **Residual:** Read access to all installed repositories for the
  token lifetime; revocation requires uninstalling the app per account.
  Broker state is durable (SQLite, F1 fixed 2026-11-22), so a broker
  restart no longer ends pairings or erases the audit history.

## 6. Repository path manipulation

- **Threat:** A malicious owner/name (e.g. `../../etc`, `CON`,
  `a/b`) escapes the storage root or overwrites unrelated data.
- **Mitigation (verified 2026-11-22):** `git-engine::paths`
  validates every component (no separators, no `..`, no reserved
  names, no illegal chars) and validates the final path stays under
  the storage root; collisions are detected before any write.
  Delete/move/scan paths canonicalize and refuse anything outside
  the canonicalized root (`crates/git-engine/src/storage.rs`).
- **Residual:** A symlink pre-placed inside the archive tree by a
  same-user attacker could redirect a clone destination; the clone
  path itself relies on component validation plus the same-user
  boundary (§1). Canonicalization at every use time is a hardening
  item, not yet done for the clone path.

## 7. Malicious repository content

- **Threat:** A synced repo contains malicious hooks, symlinks, or
  crafted objects that execute on checkout or inspection.
- **Mitigation (verified 2026-11-22):** Mirrors are bare (no
  checkout, no hook execution); working clones never auto-execute
  hooks; RepoNyx never runs repo content; Git itself is invoked
  with a controlled environment.
- **Residual:** Opening a working clone in another tool may execute
  its hooks; that is outside RepoNyx's control and is documented
  to users in the README ("Security notes").

## 8. Git process injection

- **Threat:** Crafted repo names/URLs inject extra Git flags or shell
  commands.
- **Mitigation (verified 2026-11-22):** Argument arrays only, no
  shell; URLs validated and passed as single argv elements; `--`
  separators where applicable; controlled environment
  (`GIT_ASKPASS` removed from the child environment,
  `crates/git-engine/src/process.rs`); timeouts.
- **Residual:** A Git vulnerability itself (e.g. in object parsing)
  is inherited from the installed Git version; keep Git updated.

## 9. IPC abuse

- **Threat:** A local process impersonates the desktop and commands
  the agent (mass sync, delete, exfiltrate paths).
- **Mitigation (verified 2026-11-22):** Every IPC message carries
  the per-install shared secret (see §1); strict message validation;
  versioned protocol; the protocol has no delete operation at all
  (`ClientMessage`: Ping, AgentStatus, Enqueue, CancelJob, Shutdown)
  — destructive actions are Tauri commands from the UI only.
- **Residual:** Same-user local attackers remain a residual risk (§1).

## 10. Local disk theft

- **Threat:** The machine/disk is stolen; the archive (all repos,
  history, LFS) is readable.
- **Mitigation:** None in-app; the documented control is OS
  full-disk encryption (BitLocker/FileVault). RepoNyx must not claim
  the archive is protected without it. This is stated to users in
  the README ("Security notes").
- **Residual:** Full archive disclosure if the disk is unencrypted.
  Future: optional at-rest encryption of the archive is a product
  decision, not assumed.

## Phase 10 findings

Findings from the 2026-11-22 review. Each is either fixed, or
documented here with its consequence — nothing is silently dropped.

- **F1 — Broker state is in-memory only (fixed 2026-11-22).**
  Installations, pairing codes, setup `state` values, device
  tokens, and the audit log are now persisted in SQLite
  (`broker.sqlite` under `REPONYX_BROKER_DATA_DIR`, default
  `broker-data`; `apps/auth-broker/src/store.rs`, migration
  `0001_broker_store.sql`). `BrokerState` loads the store at
  startup and writes every mutation through to it first, so a
  restart resumes where the broker left off — proven by
  `apps/auth-broker/tests/store_persistence.rs` (6 tests: pairing
  flow, revocation, single-use state, wrong-device attempts, and
  expiry pruning across a restart). Only hashes are stored, never
  raw codes/tokens and never the private key. Residuals: the
  minted-token cache and the rate-limiter buckets are still
  in-memory (a restart re-mints tokens and resets the buckets);
  audit events older than 90 days are pruned at startup.
- **F2 — Windows named pipe has no ACL (documented).** The agent's
  pipe (`\\.\pipe\reponyx-agent`) is created without a security
  descriptor; any same-user process can connect, and the token
  check is the only gate. Practical impact is bounded by the
  same-user boundary (§1) — a same-user attacker can already read
  the credential store — but restricting the pipe to the current
  user is a hardening item for a later release.
- **F3 — No key-rotation / incident runbook (fixed 2026-11-23).**
  The runbook is `docs/security/key-rotation-runbook.md`: routine
  rotation for the GitHub App private key, broker setup token,
  device tokens, IPC shared secret, and fine-grained PATs, plus
  incident response for a suspected broker, desktop/agent, or key
  compromise, with the audit queries that prove what happened.

## 11. Account revocation

- **Threat:** An installation is revoked or a PAT is deleted while
  syncs are running; jobs fail confusingly or keep using dead tokens.
- **Mitigation:** Revocation is detected (`AuthenticationRevoked`,
  non-retryable), the account is marked revoked, jobs stop, and the
  local archive is preserved (marked disconnected) — never deleted.
- **Residual:** Jobs already in flight may fail mid-operation; the
  queue must handle partial state idempotently (Phase 6).

## 12. Token expiration

- **Threat:** Installation tokens (~1 h) or PATs expire mid-sync;
  jobs fail or, worse, are retried forever.
- **Mitigation:** Fresh tokens minted per operation batch; expiration
  detected and surfaced with a reconnect action; permanent auth
  failures are never retried indefinitely (exponential backoff with
  jitter applies only to transient failures).
- **Residual:** Long mirror fetches may still cross a token boundary;
  credential refresh mid-operation is a Phase 6 design point.
