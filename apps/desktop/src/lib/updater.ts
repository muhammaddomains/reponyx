// In-app updates (audit Phase 4, E3). The updater plugin is
// configured in tauri.conf.json (GitHub releases `latest.json` +
// minisign public key) and registered in the Tauri backend, but
// nothing ever called it — this module is the first caller. Checks
// are manual from the Settings page: there is no automatic check on
// startup yet, because an update found at startup would have
// nowhere visible to go (no global notice surface — that is
// Phase 5), and a silent check that can only report "an update
// exists" in the Settings page is worse than an explicit one.

import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";

export type { DownloadEvent, Update };

export type UpdateCheckResult =
  | { status: "up-to-date" }
  | { status: "available"; update: Update }
  | { status: "failed"; message: string };

/// Ask the configured endpoint whether a newer release exists.
/// Never throws: outside Tauri (plain-browser preview) the invoke
/// fails, and before the first release the manifest itself 404s —
/// both come back as `failed` with the message, so the Settings
/// page can show them instead of crashing.
export async function checkForUpdates(): Promise<UpdateCheckResult> {
  let update: Update | null;
  try {
    update = await check();
  } catch (e: unknown) {
    return { status: "failed", message: updateErrorMessage(e) };
  }
  if (update === null) {
    return { status: "up-to-date" };
  }
  return { status: "available", update };
}

/// Download the update found by [`checkForUpdates`] and install it.
/// Installing restarts the app (restartAfterInstall defaults to
/// true on Windows). The caller owns the progress events: `Started`
/// carries the total size, `Progress` each chunk, `Finished` the
/// end of the download.
export async function downloadAndInstall(
  update: Update,
  onEvent: (event: DownloadEvent) => void,
): Promise<void> {
  await update.download(onEvent);
  await update.install();
}

export function updateErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}
