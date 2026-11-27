import { createTranslator, type Messages, type Translator } from "./translate";
import { type Locale } from "./locales";

/** Starter domain sets — more domains are added as screens land. */
export const WEB_DOMAINS = ["common", "auth", "dashboard", "errors"] as const;
export const DESKTOP_DOMAINS = ["common", "errors"] as const;
export type WebDomain = (typeof WEB_DOMAINS)[number];
export type DesktopDomain = (typeof DESKTOP_DOMAINS)[number];
export type AppSurface = "web" | "desktop";

const registry = new Map<string, Messages>();

function registryKey(locale: Locale, app: AppSurface, domain: string): string {
  return `${locale}:${app}:${domain}`;
}

export function registerTranslations(
  locale: Locale,
  app: AppSurface,
  domain: string,
  messages: Messages,
): void {
  registry.set(registryKey(locale, app, domain), messages);
}

export function getTranslator(locale: Locale, app: AppSurface, domain: string): Translator {
  const messages = registry.get(registryKey(locale, app, domain));
  if (messages === undefined) return createTranslator({});
  return createTranslator(messages);
}

/** Lazy-load one domain file (used by clients that don't statically import). */
export async function loadLocaleDomain(
  locale: Locale,
  app: AppSurface,
  domain: string,
): Promise<Messages> {
  const mod = await import(`../locales/${locale}/${app}/${domain}.json`, {
    with: { type: "json" },
  });
  const messages = (mod as { default: Messages }).default;
  registerTranslations(locale, app, domain, messages);
  return messages;
}
