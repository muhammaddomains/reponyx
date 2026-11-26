# Central broker — plan (2026-11-26, Phase 6)

## The problem (user's words)

"User sirf install kra or uska pass option ah jye pat se krna ya
auth se. bs simple. bki hr cheez build in ho." — but the GitHub
App private key cannot go in the build: it is the App's master
key, and anyone with the installer could extract it and mint
tokens for all 3 of the user's GitHub accounts. PAT is already
zero-config, and Auth for the owner is one file-pick — but for
*other* users, Auth needs the key, which cannot be shared.

## Recommendation: one owner-hosted broker, team desktops pair to it

The broker runs **once**, on a small VPS (or the owner's
always-on machine), holding the key. It serves HTTPS at a fixed
address built into the app as the default (e.g.
`https://broker.reponyx.bond` — an address is not a secret).
Team desktops just install, pick Auth, and request pairing; the
owner approves once from their own desktop, the device gets its
token, and the owner can revoke any device at any time. Local
mode stays the default for the owner; remote mode is a setting.

## How it works

### 1. The broker, hosted — no TLS code in the broker

- **Caddy in front** (automatic HTTPS via Let's Encrypt, cert
  renewal included). The broker itself stays plain HTTP on
  `127.0.0.1:8765` *inside* the VPS — Caddy owns the public
  port 443 and the certificate. No new TLS dependency in the
  broker crate (no rustls / axum-server today; the desktop's
  `BrokerClient` already accepts `https://` URLs via reqwest,
  so no client TLS work either).
- **systemd unit**, restart on boot; firewall: only 443 (plus
  SSH, key-only, no passwords).
- **Backup:** encrypted copy of `broker-data` (`broker.sqlite`)
  + the `.pem`, stored off the box. The key-rotation runbook
  (F3) already exists and gets a central-broker section.
- **Cost (Nov 2026 prices — verify at purchase):** Hetzner
  CX22 ~€3.79/mo ex VAT (~€4.50 incl VAT, ~$4.59), 2 vCPU /
  4 GB RAM / 40 GB NVMe / 20 TB traffic; DigitalOcean Basic
  $6/mo is the dearer fallback. Domain: `broker.reponyx.bond`
  costs $0 (subdomain of the domain already owned) — or
  `reponyx.com`, ~$12/yr on Porkbun, if a clean domain is
  wanted. Total: roughly $55–70/yr.
- **Alternative transport — Tailscale:** the broker stays off
  the public internet entirely; every team member joins the
  owner's tailnet and reaches the broker at its private
  address. Most private, no domain or certificate needed — but
  every member must install + join Tailscale, so it is not
  "just install RepoNyx". Caddy + VPS is the default; Tailscale
  is the option for a closed team.

### 2. Remote pairing — the one new flow (broker)

Today *every* pairing endpoint needs the setup token, which a
team device must never hold. New flow:

- `POST /pairing/remote/request` — **no setup token**,
  strictest per-IP rate limit. The device sends device name +
  identifier + installation id; the broker creates a **PENDING**
  request (new table, migration `0004`) and returns a request
  id. Responses are generic — they never confirm or deny whether
  an installation exists.
- Owner approves from their desktop: `GET /pairing/pending` +
  `POST /pairing/approve` / `POST /pairing/deny` — setup-token
  auth, the existing pattern.
- The team device polls `POST /pairing/remote/status` with its
  request id; on approval it receives the device token — the
  same 90-day token, stored as a hash only, exactly like local
  pairing. Pending requests expire (24 h) and are pruned.
- **Revocation already exists** (`POST /pairing/revoke`,
  setup-token auth): the owner revokes a device and its token
  stops minting immediately.
- **Installation sync:** the broker can already list
  installations from GitHub (`list_app_installations`, JWT →
  `GET /app/installations`). Add a sync (setup-token auth) so a
  broker learns installations without the install callback —
  this lets local + central coexist under one App, and lets the
  central broker pick up an install even if the callback was
  missed.
- **GitHub App Setup URL (owner action, GitHub UI):**
  `https://<broker-host>/github/callback`. One App has one
  Setup URL, so installs land on the central broker from then
  on.

### 3. Desktop remote mode

- New settings `broker.mode` (`local` | `remote`) +
  `broker.remote_url` (default = the built-in address,
  overridable) — added to the settings allowlist (7 keys → 9).
- **Remote mode:** no local broker spawn; the desktop just
  talks to the URL. Pairing UI: request → "waiting for owner
  approval" → token. Broker status shows remote + reachable.
  Remote URLs must be `https://` — the desktop refuses
  `http://` for remote mode.
- **Local mode unchanged** (default: spawn, loopback, no TLS).
- **PAT path untouched.**

### 4. Owner admin (desktop)

- Pending-approvals list + Approve / Deny on the Accounts page
  (owner only — the setup token stays on the owner's machine).
- Devices list + Revoke (endpoint exists); audit-events view
  (endpoint exists — needs a UI surface, even a simple list).

## Security — what changes, what doesn't

- **Doesn't change:** the key never leaves the broker; the
  desktop never sees it; device tokens expire after 90 days;
  per-device revocation; setup-token lockout (10 wrong → 15 min
  429); hashes only in the store; the audit log.
- **Changes:** the broker is now network-reachable, so TLS is
  mandatory (Caddy); a team device never holds the setup token;
  the new unauthenticated endpoint gets the strictest rate limit
  + request expiry; the VPS gets firewall + SSH-key-only +
  automatic security updates.
- **New risk, stated plainly:** the VPS holds the master key.
  If the VPS is compromised, the key is compromised — the same
  as a compromised owner PC today, but the VPS is
  network-facing, so hardening + backups + the rotation runbook
  matter more. **Availability:** if the broker is down, team
  devices cannot mint new installation tokens, so their syncs
  pause until it is back. No data loss — the queue retries; the
  repos are already on their machines.

## Rollout (phases)

- **C1 — broker:** migration `0004` (pending requests) +
  remote-pairing endpoints + installation sync + tests
  (`cargo test`; broker tests already exist).
- **C2 — hosting:** VPS + domain + Caddy + systemd + firewall
  + backup; the App Setup URL switch (owner, GitHub UI).
- **C3 — desktop:** settings + remote mode + pairing UI +
  status; frontend tests.
- **C4 — owner admin:** approvals + devices + audit UI.
- **C5 — verification:** two-machine E2E (owner approves a real
  team laptop), docs (broker README + user guide), key-rotation
  runbook update.
- Verification per phase uses the established gates: `fmt`,
  clippy with `-D warnings`, `cargo test`, frontend tests,
  Semgrep (exact CI command), MinGW where relevant;
  CI windows-build is the desktop check.

## Open questions (for the user, before implementation)

1. **Hosting:** Hetzner VPS (~€4.50/mo) vs the owner's
   always-on PC vs Tailscale mesh (no public exposure, but every
   member joins the tailnet)?
2. **Address:** `broker.reponyx.bond` (existing domain) or a new
   domain?
3. **Approvals:** owner-only, or should approval be delegable to
   someone else? (The plan assumes owner-only.)
