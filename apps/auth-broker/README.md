# Auth Broker

The Auth Broker is the **local key agent** for GitHub App auth.
Since W4 it holds **no private key at all**: it mints short-lived
installation access tokens by asking the web
(`POST /api/broker/mint`, which holds the key server-side and
mints only for installations linked to the signed-in user), runs
the device-pairing flow, and audits auth events. It is a separate
binary — it is **not** a dependency of the desktop app, and no key
material must ever appear in the desktop binary, the frontend,
local config, or logs.

## Running it (local dev)

```bash
# The web to mint through (default https://reponyx.bond):
export REPONYX_WEB_URL="https://reponyx.bond"
export REPONYX_BROKER_SETUP_TOKEN="$(openssl rand -hex 32)"
export REPONYX_BROKER_BIND="127.0.0.1:8765"
cargo run -p reponyx-auth-broker
```

The broker refuses to start without a setup token of at least 16
characters. The web session it mints with comes from the OS
credential store (`reponyx:web` / `session`, written by the
desktop on login) — the desktop passes no token in argv or env.
State is **durable SQLite** (`broker.sqlite` under
`REPONYX_BROKER_DATA_DIR`, default `broker-data`): restarting
the broker keeps installations, pairing codes, device tokens,
and the audit log.

## Protecting the broker's secrets

- **The `.env` file must not be world-readable.** The broker
  reads a repo-root `.env` at startup (real OS env vars win over
  the file; `REPONYX_BROKER_ENV_FILE` points at a different file
  when the app is installed). On Unix the broker warns at startup
  if that file is readable by other users — keep it `chmod 600`
  (owner read/write only). The file holds the setup token; anyone
  who can read it can pair a device to the local broker.
- **The broker binds loopback only by default.** Setting
  `REPONYX_BROKER_BIND` to a non-loopback address refuses to
  start unless `REPONYX_BROKER_ALLOW_NON_LOOPBACK=true` is also
  set. Only set that on a trusted network — anyone on the network
  can then reach the broker, and the setup token is the only
  thing standing between them and your installations.

## Endpoints

| Method | Path | Auth | Purpose |
| ------ | ---- | ---- | ------- |
| GET | `/health` | none | liveness (`{"status":"ok"}`) |
| POST | `/setup/state` | setup token | issue a one-time CSRF `state` + the GitHub install URL |
| GET | `/github/callback` | `state` query param | GitHub App install callback; validates `state` (single-use, 10 min), records the installation |
| POST | `/pairing/request` | setup token | request a pairing code for an installation (10 min TTL) |
| POST | `/pairing/confirm` | setup token | confirm a code → receive a device token |
| POST | `/pairing/status` | setup token | pairing-code status (code in the JSON body, never in a URL query) |
| POST | `/pairing/revoke` | setup token | revoke a device's codes and tokens |
| POST | `/installations/{id}/token` | device token | mint an installation access token via the web (cached until 5 min before expiry) |
| GET | `/installations` | setup token | list recorded installations |
| GET | `/audit` | setup token | audit events |

Errors are `{ "error": { "code": "...", "message": "..." } }` using
RepoNyx error codes.

## Pairing flow

1. The operator (desktop) holds the broker setup token — supplied
   at broker startup via `REPONYX_BROKER_SETUP_TOKEN` and stored in
   the desktop's OS credential store. It authenticates the pairing
   request: `X-RepoNyx-Setup-Token`.
2. The desktop asks the broker for a pairing code for an
   installation (the installation must already be recorded via the
   install callback). The code is 9 characters from an unambiguous
   alphabet, shown as `XXX-XXX-XXX`, single-use, and it does not
   expire (spent codes are pruned after 90 days).
3. The user confirms the code (the desktop sends it back with the
   device name + identifier). The broker checks the code, its
   expiry, and that the device identifier matches the one the code
   was issued for; 5 wrong-device attempts invalidate the code.
4. On success the broker issues a random 32-byte device token. The
   desktop stores it in the OS credential store and presents it as
   `Authorization: Bearer <device_token>` when minting installation
   tokens. The broker stores only the SHA-256 hash of the token.
   **A device token lives 90 days** (`device_token_ttl`); after
   that, minting fails with "device token expired; pair this
   device again" and the device must re-pair. **Re-pairing revokes
   the device's previous tokens** — only the latest token works.
5. Revocation (`POST /pairing/revoke`) invalidates the device's
   tokens and pending codes.

## Rate limiting

Hand-rolled per-key token buckets (no extra dependency): token
minting is limited per device (30/min), pairing requests per IP
(10/min), pairing confirms per IP (20/min), the install callback
per IP (10/min), and setup-state issuance per IP (10/min).
Pairing codes also have a 5-attempt lockout of their own. Limits
return HTTP 429 with `Retry-After`.

**Setup-token lockout:** 10 consecutive failed setup-token
presentations lock setup-token auth out for 15 minutes (429 with
`Retry-After`); a success resets the counter. The lockout lives
in memory — a broker restart resets it — and individual failures
are not audited, so a guessing loop cannot flood the audit log;
only the lockout transition is.

## Audit

Append-only, durable (SQLite, 90-day retention): pairing
requested/confirmed/revoked/failed, token minted/failed, callback
received/failed, auth failures. Events carry timestamp, actor
(device id, `setup`, or client IP), action, outcome, and detail
— never a secret, token, code, or key. A test proves a minted
token never appears in the audit output or broker logs.

## Token minting

The broker signs a GitHub App JWT (RS256, `iss` = App id,
`iat` = now − 60 s, `exp` = now + 9 min) and calls
`POST /app/installations/{id}/access_tokens`. Minted tokens are
cached per installation until 5 minutes before expiry; a caller
that gets a 401 can ask for `force_refresh: true`.

## What is NOT done here (needs user approval)

- Publishing or creating the GitHub App — the App id/slug/key are
  operator-supplied; nothing in this repo creates an App.
- Deploying the broker anywhere — it binds loopback by default.
- Any production credential: tests use throwaway keys generated
  in-memory.
