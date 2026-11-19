# Auth Broker

The Auth Broker is the **only** component that holds the GitHub App
private key. It mints short-lived installation access tokens for
paired devices, runs the device-pairing flow, and audits auth
events. It is a separate binary — it is **not** a dependency of the
desktop app, and the private key must never appear in the desktop
binary, the frontend, local config, or logs.

## Running it (local dev)

```bash
export REPONYX_BROKER_APP_ID="123456"
export REPONYX_BROKER_APP_SLUG="reponyx"
# One of the two key options (never commit the file):
export REPONYX_BROKER_PRIVATE_KEY_PATH="$HOME/.config/reponyx/broker-key.pem"
export REPONYX_BROKER_SETUP_TOKEN="$(openssl rand -hex 32)"
export REPONYX_BROKER_BIND="127.0.0.1:8765"
cargo run -p reponyx-auth-broker
```

The broker refuses to start without the App id, the private key,
and a setup token of at least 16 characters. State is held
**in memory** in Phase 5: restarting the broker forgets
installations, pairing codes, and device tokens, so devices must
re-pair. Durable broker storage is a hardening item for the
deployment phase.

## Endpoints

| Method | Path | Auth | Purpose |
| ------ | ---- | ---- | ------- |
| GET | `/health` | none | liveness (`{"status":"ok"}`) |
| POST | `/setup/state` | setup token | issue a one-time CSRF `state` + the GitHub install URL |
| GET | `/github/callback` | `state` query param | GitHub App install callback; validates `state` (single-use, 10 min), records the installation |
| POST | `/pairing/request` | setup token | request a pairing code for an installation (10 min TTL) |
| POST | `/pairing/confirm` | setup token | confirm a code → receive a device token |
| GET | `/pairing/status?code=…` | setup token | pairing-code status |
| POST | `/pairing/revoke` | setup token | revoke a device's codes and tokens |
| POST | `/installations/{id}/token` | device token | mint an installation access token (cached until 5 min before expiry) |
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
   alphabet, shown as `XXX-XXX-XXX`, single-use, 10-minute TTL.
3. The user confirms the code (the desktop sends it back with the
   device name + identifier). The broker checks the code, its
   expiry, and that the device identifier matches the one the code
   was issued for; 5 wrong-device attempts invalidate the code.
4. On success the broker issues a random 32-byte device token. The
   desktop stores it in the OS credential store and presents it as
   `Authorization: Bearer <device_token>` when minting installation
   tokens. The broker stores only the SHA-256 hash of the token.
5. Revocation (`POST /pairing/revoke`) invalidates the device's
   tokens and pending codes.

## Rate limiting

Hand-rolled per-key token buckets (no extra dependency): token
minting is limited per device (30/min), pairing requests per IP
(10/min), pairing confirms per IP (20/min). Pairing codes also
have a 5-attempt lockout of their own. Limits return HTTP 429
with `Retry-After`.

## Audit

Append-only, in-memory: pairing requested/confirmed/revoked/
failed, token minted/failed, callback received/failed, auth
failures. Events carry timestamp, actor (device id, `setup`, or
client IP), action, outcome, and detail — never a secret, token,
code, or key. A test proves a minted token never appears in the
audit output or broker logs.

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
