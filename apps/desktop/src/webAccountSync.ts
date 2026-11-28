/** Dispatched on `window` after a web-account link changes — an account
 *  is linked or unlinked — so the app re-fetches package, limits and
 *  usage at once instead of waiting for the next 60s refresh tick
 *  (W4-full, 2026-11-28: real-time two-way app ↔ web sync). */
export const WEB_ACCOUNT_CHANGED_EVENT = "reponyx:account-changed";

export function notifyWebAccountChanged(): void {
  window.dispatchEvent(new Event(WEB_ACCOUNT_CHANGED_EVENT));
}
