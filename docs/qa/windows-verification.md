# Windows verification checklist (Phase 10)

Run these on a real Windows machine (the user's
`D:\github\zaheer\reponyx\reponyx-main` clone or an installed
release build). Nothing here can run on the Linux dev VM — the
items that were verifiable there are listed under §6 instead.

Prerequisites: Windows 10/11, MSVC Build Tools, WebView2 runtime,
Git + Git LFS, Node 26+, Rust stable. Repo at
`D:\github\zaheer\reponyx\reponyx-main`.

## 1. Installer (NSIS)

Config: `apps/desktop/src-tauri/tauri.conf.json` (`bundle.windows.nsis`,
per-user install, no admin needed) and `.github/workflows/release.yml`
(tauri-action, NSIS setup exe + portable ZIP + signed updater
artifacts).

1. Build locally: `cargo tauri build` in `apps/desktop`
   (or download the `RepoNyx_<version>_x64-setup.exe` from a draft
   release — requires `TAURI_SIGNING_PRIVATE_KEY` (+ password) in
   repo secrets first).
2. Run the installer. Expect: per-user install, no UAC prompt,
   Start Menu entry, desktop shortcut if offered.
3. Launch from the Start Menu. Expect: the app opens, the Agent
   page shows the agent reachable.
4. Uninstall from Settings → Apps. Expect: clean removal, no
   leftover `RepoNyx` folder under `%LOCALAPPDATA%` (user data
   under `%APPDATA%\RepoNyx` is kept by design — confirm it is
   still there).
5. Upgrade: install the new version over the old one. Expect: the
   app starts, the database migrates (V1→V4 upgrade tests cover
   the migrations), repositories and schedules are intact.

## 2. Task Scheduler — start at logon

Wiring (verified in code, 2026-11-23): the Agent page has
"Enable start at login" / "Disable start at login" buttons →
`registerAgentAutostart` / `unregisterAgentAutostart` in
`packages/api-client` → Tauri commands in
`apps/desktop/src-tauri/src/agent.rs` → `schtasks /Create /TN
RepoNyxBackgroundAgent /TR "<agent exe>" /SC ONLOGON /RL LIMITED
/F`. The old duplicate helpers in the agent's `windows_task.rs`
were deleted (zero callers).

1. In the app: Agent page → **Enable start at login**. Expect: no
   error (the command returns "Registered
   RepoNyxBackgroundAgent (ONLOGON)").
2. In PowerShell: `schtasks /Query /TN RepoNyxBackgroundAgent`.
   Expect: the task exists, trigger `ONLOGON`, run level
   `LIMITED`.
3. Sign out and sign back in. Expect: the agent is running without
   opening the app (Agent page shows it reachable; or
   `Get-Process reponyx-agent` finds it).
4. Agent page → **Disable start at login**. Expect:
   `schtasks /Query /TN RepoNyxBackgroundAgent` now reports
   "ERROR: The system cannot find the file specified."
5. Known limit (ADR-006): a user-session task does not survive
   logout — the agent stops at logout and restarts at logon. A
   true Windows Service is out of scope.

## 3. Credential Manager

Secrets live in the OS credential store only, via the `keyring`
crate (v4): PATs under service `reponyx:pat`, the IPC shared
secret under `reponyx:ipc` / `shared-secret`, the broker setup
token under `reponyx:broker` / `setup-token`, device tokens under
`reponyx:broker` / `device-token: <redacted>`.

1. Pair a GitHub App account (or add a PAT) in the app.
2. Open Credential Manager → Windows Credentials. Expect: the
   entries above are present (values are secrets — never paste
   them anywhere).
3. Restart the desktop app. Expect: the account still works
   (Discover succeeds) without re-entering anything.
4. Remove a PAT from the Accounts page. Expect: the entry is gone
   from Credential Manager and the account shows revoked.

## 4. Binary secret scan

`scripts/check-no-private-key.sh` proves the GitHub App private
key is absent from the desktop app. On the Linux VM only the
source half runs (no built binary exists there) — it passes. On
Windows, run the full script after `cargo tauri build`:

```powershell
bash scripts/check-no-private-key.sh
```

Expect: `check-no-private-key: PASSED` **and** the binary section
must actually scan (not `SKIP`) — it checks
`apps/desktop/src-tauri/target/release/reponyx-desktop` for the
`BEGIN PRIVATE KEY` markers. If it says `SKIP`, the binary was
not built in the expected location; fix the path before release.

## 5. Process-kill recovery (the real one)

The lease mechanism is proven at the DB layer only
(`lease_expiry_drives_recovery`,
`recovery_requeues_abandoned_running_jobs` in sync-queue). This
is the end-to-end proof:

1. Start a sync of a large repository (a real mirror clone that
   takes at least a few seconds).
2. While it is running: `taskkill /F /IM reponyx-agent.exe`.
3. Start the agent again (open the app → Agent page → Start
   agent, or sign out/in if autostart is on).
4. Expect: the interrupted job is requeued after its lease
   expires (default lease window — see `crates/sync-queue`), the
   clone resumes/completes, and the repository verifies healthy
   (no half-written state left behind). Record the actual
   timings; if recovery takes longer than the lease window,
   that is a finding, not a pass.

## 6. Already verified on the Linux VM (2026-11-23)

- Secret scan, source half: `check-no-private-key.sh` PASSED
  (no key markers in desktop sources, no broker-key references,
  no `reponyx-auth-broker` in the desktop dependency tree, no
  committed `.pem`/`.key` under `apps/desktop`). Binary half
  SKIPPED — needs §4.
- Bundle config: NSIS `currentUser` + WiX `en-US`, all icons
  present (`32x32.png`, `128x128.png`, `128x128@2x.png`,
  `icon.icns`, `icon.ico`).
- Release workflow: tauri-action with `TAURI_SIGNING_PRIVATE_KEY`
  (+ password) from repo secrets; ships NSIS setup exe, portable
  ZIP, signed updater artifacts. **Not set yet** — set the
  secrets before the first release, or the build fails at
  signing.
- Autostart wiring: Agent page buttons → api-client →
  registered Tauri commands → schtasks (Windows) /
  systemd user unit (Linux) / LaunchAgent (macOS). Dead
  duplicate `windows_task.rs` in the agent removed.
- `cargo audit` clean (cookie 0.18.3 fix, 2026-11-23); CI green
  on the pushed head.

## 7. Open after this checklist

- First release (needs §1 step 1's signing secrets + a release
  tag; the Labeler workflow also needs the 7 labels created in
  repo Settings).
- Mid-transfer network loss: no test drops a connection halfway
  through a fetch — git surfaces the error and the queue retries
  it, but the exact sequence is unproven (see
  `docs/qa/failure-recovery.md`).
