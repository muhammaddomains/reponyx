# Deployment Architecture

Status: Accepted (Phase 1) · Release: Phase 10

## Desktop (Windows-first)

- Built with Tauri 2: NSIS installer on Windows; `.dmg` (macOS) and
  `.deb`/`.AppImage` (Linux) later.
- Windows prerequisites: MSVC Build Tools, WebView2 runtime (Evergreen).
- Linux build prerequisites: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`,
  `libayatana-appindicator3-dev`, `librsvg2-dev`, `libsoup-3.0-dev`.
- Default storage root `D:\RepoNyx\` on Windows (configurable); the
  database lives under the app-data directory
  (`%APPDATA%\RepoNyx\data\reponyx.sqlite` on Windows,
  `~/.local/share/RepoNyx/data/reponyx.sqlite` on Linux).

## Auth Broker (Phase 5)

- Separately hosted service (not shipped with the desktop).
- Holds the GitHub App private key in its environment / secret manager.
- Exposes: installation callback, token minting, device pairing,
  audit log. Rate-limited; TLS only.
- No desktop release may contain the private key. CI must fail if a
  private key or token is found in the desktop artifact
  (secret scanning, Phase 10).

## Background agent (Phase 7)

- Ships inside the desktop installer as a separate binary.
- Registered at login via Windows Task Scheduler (user session).
- A native Windows Service is a later option; its credential-store
  implications must be resolved first (see background-agent doc).

## Updates

- Desktop updates via the installer / Tauri updater (Phase 10).
- Agent update compatibility: the agent and desktop negotiate IPC
  protocol version; a mismatched agent is reported, not silently used.
- Database migrations run on open; downgrades are not supported —
  back up before upgrading.

## Environments

| Environment | Purpose |
| --- | --- |
| Dev (this repo) | `pnpm dev` / `cargo tauri dev` |
| CI (Phase 10) | fmt, clippy, tests, typecheck, lint, builds, audits |
| Release | Signed installers, changelog, migration notes |
