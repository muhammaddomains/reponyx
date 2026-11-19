# Threat Model

Status: Phase 1 draft · Review: Phase 10 security audit

Scope: RepoNyx desktop + background agent + Auth Broker + GitHub
integration. Out of scope: GitHub's own infrastructure, the user's
operating system, and the physical security of GitHub's data centers.

Security is not claimed because "encryption is used". Each threat
below names the boundary, the mitigation, and the residual risk.

## 1. Malicious local processes

- **Threat:** Another process on the machine reads the SQLite DB,
  scrapes memory, or issues IPC commands to trigger syncs/deletes.
- **Mitigation:** IPC ACLs + desktop authentication (Phase 7); no
  network listener; DB file under the user's profile with OS file
  permissions; no secrets in the DB.
- **Residual:** A process running as the same user can read the user's
  files. RepoNyx cannot defend against same-user malware; OS account
  separation is the boundary.

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
- **Mitigation:** Key exists only in the broker environment; never in
  the desktop binary, repo, logs, or CI artifacts; secret scanning in
  CI; short-lived tokens; least-privilege permissions.
- **Residual:** Compromise of the broker host is total for installed
  accounts until the key is rotated and installations re-authorized.
  Key rotation + incident runbook are Phase 5/10 items.

## 4. Unauthorized device pairing

- **Threat:** An attacker pairs their device to a victim's GitHub
  installation and receives its repositories.
- **Mitigation:** Cryptographically random, single-use, short-lived
  pairing codes; rate limiting; device identification; explicit user
  confirmation in the browser flow; revocation + audit history.
- **Residual:** If an attacker also controls the victim's GitHub
  session, pairing cannot distinguish them; GitHub session security
  is the boundary.

## 5. Compromised Auth Broker

- **Threat:** Attacker controls the broker: mints tokens, harvests
  installation metadata, or serves malicious pairing flows.
- **Mitigation:** Least-privilege app permissions; tokens expire in
  ~1 h; audit logs; rate limits; TLS; broker has no access to local
  archives (it never sees repository contents).
- **Residual:** Read access to all installed repositories for the
  token lifetime; revocation requires uninstalling the app per account.

## 6. Repository path manipulation

- **Threat:** A malicious owner/name (e.g. `../../etc`, `CON`,
  `a/b`) escapes the storage root or overwrites unrelated data.
- **Mitigation:** `git-engine::paths` validates every component
  (no separators, no `..`, no reserved names, no illegal chars) and
  validates the final path stays under the storage root; collisions
  are detected before any write.
- **Residual:** Symlink attacks on an already-created path need
  canonicalization checks at use time (Phase 3 hardening).

## 7. Malicious repository content

- **Threat:** A synced repo contains malicious hooks, symlinks, or
  crafted objects that execute on checkout or inspection.
- **Mitigation:** Mirrors are bare (no checkout, no hook execution);
  working clones never auto-execute hooks; RepoNyx never runs repo
  content; Git itself is invoked with a controlled environment.
- **Residual:** Opening a working clone in another tool may execute
  its hooks; that is outside RepoNyx's control and must be documented
  to users.

## 8. Git process injection

- **Threat:** Crafted repo names/URLs inject extra Git flags or shell
  commands.
- **Mitigation:** Argument arrays only, no shell; URLs validated and
  passed as single argv elements; `--` separators where applicable;
  controlled environment (no `GIT_ASKPASS` inheritance); timeouts.
- **Residual:** A Git vulnerability itself (e.g. in object parsing)
  is inherited from the installed Git version; keep Git updated.

## 9. IPC abuse

- **Threat:** A local process impersonates the desktop and commands
  the agent (mass sync, delete, exfiltrate paths).
- **Mitigation:** Named-pipe ACLs, per-install authentication secret,
  strict message validation, versioned protocol; destructive actions
  (delete) always require UI confirmation and are never IPC-triggerable
  without it.
- **Residual:** Same-user local attackers remain a residual risk (§1).

## 10. Local disk theft

- **Threat:** The machine/disk is stolen; the archive (all repos,
  history, LFS) is readable.
- **Mitigation:** None in-app in Phase 1; the documented control is
  OS full-disk encryption (BitLocker/FileVault). RepoNyx must not claim
  the archive is protected without it.
- **Residual:** Full archive disclosure if the disk is unencrypted.
  Future: optional at-rest encryption of the archive is a product
  decision, not assumed.

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
