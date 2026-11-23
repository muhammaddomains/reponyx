# Key rotation & incident runbook (F3)

Closes threat-model finding F3. Read this before the first public
release — and before rotating any RepoNyx secret. Every procedure
below is written against the real code: the env vars and endpoints
named here are the ones the broker, agent, and desktop actually
use (see `apps/auth-broker/src/config.rs`,
`crates/ipc/src/auth.rs`, `apps/desktop/src-tauri/src/auth.rs`).

## 1. The secrets, and where they live

| Secret | Lives in | Rotated by |
|---|---|---|
| GitHub App private key (PEM) | Broker only, from `REPONYX_BROKER_PRIVATE_KEY` or `REPONYX_BROKER_PRIVATE_KEY_PATH` (read once at startup) | §2.1 |
| Broker setup token | Broker env `REPONYX_BROKER_SETUP_TOKEN` (min 16 chars); desktop copy in the OS credential store, service `reponyx:broker`, account `setup-token` | §2.2 |
| Device token (per paired device) | Broker store (SHA-256 hash only) + desktop OS credential store, service `reponyx:broker`, account `device-token: <redacted>` | §2.3 |
| IPC shared secret (desktop ↔ agent) | OS credential store, service `reponyx:ipc`, account `shared-secret` (32-byte hex, created by the agent on first start) | §2.4 |
| Fine-grained PAT (per GitHub account) | OS credential store only, service `reponyx:pat`, account = account UUID | §2.5 |
| Minted installation tokens | Broker memory only (never persisted); GitHub-side, 1-hour expiry | Rotate by restarting the broker (§2.1, step 5) |

Rules that apply to all of them:

- Secrets never go in the repo, in SQLite (except their SHA-256
  hashes in the broker store), in logs, in chat, in screenshots,
  or in GitHub issues. If a secret ever appears in one of those
  places, treat it as compromised and follow §3.
- The GitHub App private key never leaves the broker host. It is
  never copied to the desktop, the agent, or a developer machine
  except for the one-time transfer into the broker's env/file.
- Always verify after a rotation (§2.6) before deleting the old
  value. A rotation is not done until the new value is proven to
  work.

## 2. Routine rotation

### 2.1 GitHub App private key

Rotate on a schedule (yearly is a reasonable default) and
immediately if the key is ever exposed.

1. In GitHub: App settings → General → Private keys → **Generate
   a private key**. Download the new `.pem`. The old key keeps
   working until you delete it — that is what makes this safe to
   do live.
2. On the broker host, replace the key the broker reads:
   - Env var: set `REPONYX_BROKER_PRIVATE_KEY` to the new PEM
     text, or
   - File: replace the file `REPONYX_BROKER_PRIVATE_KEY_PATH`
     points at.
   Keep the old key file around (outside the broker's data dir)
   until step 5 passes.
3. Restart the broker. It reads the key once at startup and
   refuses to start its token endpoints without a valid PEM.
4. Verify (§2.6): broker `/health` answers, and a token mint
   succeeds (pair a test device, or watch the next scheduled sync
   complete for a GitHub App account).
5. In GitHub: delete the **old** private key. Only now is the old
   key dead. JWTs minted with it expire within 10 minutes anyway,
   and installation tokens within an hour.
6. If anything fails at step 4, put the old key back and restart
   the broker — the old key is still valid until step 5.

### 2.2 Broker setup token

The setup token authenticates pairing/setup requests from the
desktop operator. Rotate it when the operator changes, or when
the token is ever exposed.

1. Generate a new one: `openssl rand -hex 32`.
2. Set `REPONYX_BROKER_SETUP_TOKEN` to it on the broker host and
   restart the broker.
3. In the desktop: Accounts page → broker setup → enter the new
   token (it is stored in the OS credential store, service
   `reponyx:broker`, account `setup-token`). Pairing with the old
   token stops working immediately.
4. Verify (§2.6): start a pairing and confirm the broker accepts
   the new token.

### 2.3 Device token (one paired device)

Use this when a device is lost, sold, or its pairing is suspect.
No broker restart is needed.

1. In the desktop: Accounts page → the device → **Revoke
   pairing**. This calls the broker's `POST /pairing/revoke` with
   the device identifier and deletes the local device token from
   the OS credential store. (The broker stores only the token's
   SHA-256 hash, so revocation is a hash lookup, not a secret
   comparison.)
2. To revoke without the desktop (e.g. the device is gone):
   `POST /pairing/revoke` with the setup token and the
   `device_identifier` (see `apps/auth-broker/src/routes.rs`).
3. The device must pair again from scratch to sync. Nothing else
   on that device keeps working: its minted tokens are already
   dead (they were bound to the revoked device token).
4. Verify (§2.6): the revoked device's next sync attempt fails
   with an authentication error, and the audit log shows the
   revocation (§4).

### 2.4 IPC shared secret (desktop ↔ agent)

The agent creates this on first start (`ensure_shared_secret`,
`crates/ipc/src/auth.rs`) and the desktop reads it on every IPC
call, so rotation is a restart, not a reconfiguration.

1. Stop the agent (Agent page → Stop, or `taskkill` the agent
   process on Windows).
2. Delete the entry from the OS credential store: service
   `reponyx:ipc`, account `shared-secret` (Windows Credential
   Manager → Windows Credentials; on Linux, the Secret Service
   entry of the same name).
3. Start the agent again. It generates a fresh 32-byte secret and
   stores it; the desktop picks it up on its next IPC call — no
   desktop restart is needed.
4. Verify (§2.6): the Agent page shows the agent reachable, and a
   sync can be enqueued from the desktop.

### 2.5 Fine-grained PAT (one GitHub account)

1. In the desktop: Accounts page → the account → **Replace PAT**,
   and paste the new fine-grained PAT (it goes straight to the OS
   credential store, service `reponyx:pat`).
2. On GitHub: Settings → Developer settings → Personal access
   tokens → Fine-grained tokens → delete (or revoke) the old
   token.
3. Verify (§2.6): run Discover for that account and confirm it
   succeeds; a sync for one of its repositories completes.

### 2.6 Verify after any rotation

- Broker `/health` answers (default `http://127.0.0.1:8765/health`).
- Agent page in the desktop: agent reachable, not paused.
- One sync completes for an account that uses the rotated secret
  (GitHub App account for §2.1/§2.2/§2.3, any account for §2.4,
  the PAT account for §2.5).
- The audit log shows the expected entries (§4) and no unexpected
  pairing or mint failures.

## 3. Incident response

### 3.1 Suspected broker compromise

Signs: pairing or mint failures in the audit log you did not
cause, an unknown device in the installations list, or a secret
found outside its home (§1).

1. **Contain first.** Stop the broker process. A stopped broker
   mints nothing and pairs nothing; already-minted installation
   tokens expire within an hour on their own.
2. **Preserve evidence before rotating anything.** Copy
   `broker.sqlite` (under `REPONYX_BROKER_DATA_DIR`, default
   `broker-data`) aside, and export the audit rows (§4). Do not
   delete the data dir.
3. **Revoke everything that could be live, in this order:**
   - Every device token: `POST /pairing/revoke` per device (or
     delete the device rows — see §4 for the queries). All devices
     must re-pair afterwards.
   - The setup token: rotate per §2.2.
   - The GitHub App private key: rotate per §2.1 — and delete the
     old key in GitHub only after the new one is verified.
4. **Restart the broker** with the new key and setup token, then
   verify per §2.6.
5. **Re-pair devices one by one** from the desktop. Each pairing
   is a fresh 9-character single-use code with a 10-minute TTL,
   confirmed in the browser flow.
6. **Review the audit log** (§4) for what the attacker did:
   which devices paired, which tokens were minted, and when. If
   any repository was synced by an unknown device, treat its
   archive as untrusted until you have compared it with the
   GitHub remote.

### 3.2 Suspected desktop / agent compromise

Signs: the IPC secret or a PAT found outside the OS credential
store, or syncs you did not start.

1. Rotate the IPC shared secret (§2.4) and every PAT (§2.5).
2. Revoke every device pairing (§2.3) and re-pair only the
   devices you still control.
3. Check the desktop audit log (`audit_logs` in
   `reponyx.sqlite`) for unexpected pairings, revocations, and
   restores.

### 3.3 Suspected GitHub App key leak (key seen somewhere it should not be)

1. Rotate the key per §2.1 — generate the new key, restart the
   broker, verify, then delete the old key in GitHub. Do not wait
   for evidence of misuse: a leaked key is a compromised key.
2. Check the broker audit log for mints you did not cause (§4),
   and GitHub's App settings for installations you do not
   recognize. Uninstall the App from any installation you do not
   control.

## 4. Audit evidence

**Broker audit log** — table `broker_audit_events` in
`broker.sqlite` (`<REPONYX_BROKER_DATA_DIR>/broker.sqlite`,
default `broker-data/broker.sqlite` next to the broker binary).
Events: pairing confirmed / revoked / failed, token minted / mint
failed, and their outcomes. Rows older than 90 days are pruned at
broker startup, so copy the file before investigating anything
older than that.

```sql
-- Recent events, newest first:
SELECT timestamp, actor, action, outcome, detail
FROM broker_audit_events ORDER BY timestamp DESC LIMIT 50;

-- Pairings and revocations only:
SELECT timestamp, actor, action, outcome
FROM broker_audit_events
WHERE action LIKE '%pairing%' OR action LIKE '%revoke%'
ORDER BY timestamp DESC LIMIT 50;

-- Failed mints (possible abuse or a broken key):
SELECT timestamp, actor, action, outcome, detail
FROM broker_audit_events
WHERE action LIKE '%mint%' AND outcome != 'ok'
ORDER BY timestamp DESC LIMIT 50;
```

Run with `sqlite3 broker.sqlite "<query>"` on the broker host.
(Only hashes are stored — never raw codes or tokens — so the log
is safe to quote in a bug report.)

**Desktop audit log** — table `audit_logs` in `reponyx.sqlite`
(`<data_dir>/data/reponyx.sqlite`). Same idea: pairings,
revocations, restores, and their outcomes.

**GitHub side** — App settings → Installations (who installed the
App and when), and the account's fine-grained token list (last
used dates). A token or installation you do not recognize is
itself the finding.

## 5. What this runbook does not cover

- Broker deployment itself (where the broker runs, TLS, firewall)
  is a deployment decision, still open in Phase 10 — this runbook
  assumes the broker runs on loopback (`127.0.0.1:8765`) as
  configured.
- The Windows named pipe has no ACL (threat-model F2,
  documented): rotation of the IPC secret does not change that;
  the boundary is the same-user OS account.
- There is no automated rotation: every procedure above is manual
  and must be run by the operator. If rotation ever becomes
  automated, this runbook is the spec it must follow.
