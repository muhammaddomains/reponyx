# Updater end-to-end test (audit E3)

The auto-updater is a privileged code-execution path: it downloads a
binary from GitHub releases and installs it. It must be proven end to
end on a real Windows machine before the first release is published
— not just built. This procedure is the test. It needs two releases,
so it runs after v0.1.0 is published, not before.

## What is already proven

- The updater plugin is registered in the desktop backend and the
  `@tauri-apps/plugin-updater` package is installed; Settings →
  Advanced → Updates calls `check()`, and Download & install calls
  `download()` then `install()`. Before Phase 4 nothing ever called
  it — the wiring itself was untested.
- The configured public key in `tauri.conf.json` decodes as a valid
  minisign Ed25519 public key (key id `d54eef67f4ed1092`, checked
  2026-11-25). The key only verifies what the matching private key
  signed — see the prerequisite below.
- CI's `windows-installer` job proves the installer, the portable
  ZIP, and the updater artifacts (`latest.json`) build. It signs
  with a throwaway key generated in the job, so it does **not**
  prove the update flow: the throwaway key does not match the
  public key baked into the app, and a client would refuse that
  signature. Only this procedure proves the flow.

## Prerequisites

- `TAURI_SIGNING_PRIVATE_KEY` (+ password) set in the repo secrets
  (Settings → Secrets and variables → Actions). The private key
  must be the one whose public key is in `tauri.conf.json`
  (`plugins.updater.pubkey`). If the keypair was generated with
  `tauri signer generate`, the public key it printed is the one
  that belongs in the config — a mismatch means the client refuses
  every update. There is no way to check this from code; confirm
  it by running this procedure.
- A Windows machine with no RepoNyx installed yet (or uninstall
  first — an installed v0.1.0 must be the release build, not a
  `tauri dev` run, or the updater has nothing to update from).

## Procedure

1. **Publish v0.1.0.** Tag `v0.1.0` and push the tag. The Release
   workflow builds, signs, and creates a **draft** release. Open
   the draft and check the assets are all there: the NSIS
   installer, `latest.json`, the portable ZIP, the agent ZIP, the
   broker ZIP. Publish the release (change it from draft to
   published — the updater endpoint reads the *latest published*
   release, a draft is invisible to it).
2. **Install v0.1.0.** On the Windows machine, download the NSIS
   installer from the published release and install it. Run the
   app. Settings → Advanced shows App version `0.1.0`.
3. **Check for updates (negative).** Settings → Advanced →
   Updates → Check for updates. Expected: "You're up to date."
   (There is no newer release yet.) This proves the endpoint,
   the manifest fetch, and the signature check on the *current*
   version all work.
4. **Publish v0.1.1.** Bump the version to `0.1.1` in
   `apps/desktop/src-tauri/Cargo.toml` **and** in
   `apps/desktop/src-tauri/tauri.conf.json` (the app version comes
   from the Cargo package; the config field must match it), commit,
   tag `v0.1.1`, push the tag, and publish that release the same
   way as step 1.
5. **Check for updates (positive).** Back in the running v0.1.0
   app (do not close it): Settings → Advanced → Updates → Check
   for updates. Expected: "Version 0.1.1 is available (you're on
   0.1.0)." with the release notes shown.
6. **Download & install.** Click Download & install. Expected:
   a download progress line, then the app closes, the installer
   runs, and the app restarts on its own. Settings → Advanced
   shows App version `0.1.1`.
7. **Check again.** Check for updates once more. Expected:
   "You're up to date."

## If a step fails

- **Check says the manifest cannot be fetched:** open
  `https://github.com/muhammaddomains/reponyx/releases/latest/download/latest.json`
  in a browser. It should be the v0.1.1 manifest. A 404 means the
  release is still a draft, or the tag build failed — check the
  Actions run for that tag first.
- **Check finds the update but install refuses it:** the signature
  does not verify against the public key in `tauri.conf.json`.
  The signing key in the repo secrets is not the matching private
  key — rotate the secrets to the matching keypair and re-run
  from step 4 (a new tag is needed; a release cannot be re-signed
  in place).
- **The app does not restart after install:** the NSIS installer
  ran but the restart step failed — check whether v0.1.1 is
  installed (Add or remove programs) and launch it by hand; the
  update itself still landed.

## Pass criteria

Steps 3, 5, 6, and 7 all behave as expected, on the release
builds, with the real signing key. Until then the updater is
"wired, not proven" — the release notes must keep saying so, and
the release stays a draft until the flow passes.
