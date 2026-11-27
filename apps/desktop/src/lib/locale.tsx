/** Locale (language) for the desktop app — `ui.locale` (`en` | `ur`).
 *
 *  Mirrors the web's static-import + registerTranslations pattern:
 *  the desktop locale JSONs are bundled, reads are synchronous, and
 *  switching writes `ui.locale` and re-renders through context.
 *  `dir="rtl"` + `lang` are set on `<html>` for Urdu, and numbers
 *  stay Latin digits (the same rule as the web).
 *
 *  Error rendering: the Rust commands return the web's error code
 *  (e.g. `invalid_credentials`) in `CommandError.code`, so the
 *  frontend localizes it from `errors.json` here — the message the
 *  backend sends is only the English fallback. `{retryAt}` is
 *  formatted the same way the web formats it (medium date, short
 *  time). */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { getSettings, setSetting } from "@reponyx/api-client";
import {
  DEFAULT_LOCALE,
  SELECTABLE_LOCALES,
  getLocaleDirection,
  getTranslator,
  isSupportedLocale,
  registerTranslations,
  type DesktopDomain,
  type Locale,
  type Translator,
} from "@reponyx/i18n";
import commonEn from "@reponyx/i18n/locales/en/desktop/common.json";
import authEn from "@reponyx/i18n/locales/en/desktop/auth.json";
import errorsEn from "@reponyx/i18n/locales/en/desktop/errors.json";
import commonUr from "@reponyx/i18n/locales/ur/desktop/common.json";
import authUr from "@reponyx/i18n/locales/ur/desktop/auth.json";
import errorsUr from "@reponyx/i18n/locales/ur/desktop/errors.json";

// Register once at module load; reads are then synchronous.
registerTranslations("en", "desktop", "common", commonEn);
registerTranslations("en", "desktop", "auth", authEn);
registerTranslations("en", "desktop", "errors", errorsEn);
registerTranslations("ur", "desktop", "common", commonUr);
registerTranslations("ur", "desktop", "auth", authUr);
registerTranslations("ur", "desktop", "errors", errorsUr);

export { SELECTABLE_LOCALES };
export type { Locale };

interface LocaleContextValue {
  locale: Locale;
  setLocale: (locale: Locale) => Promise<void>;
}

const LocaleContext = createContext<LocaleContextValue>({
  locale: DEFAULT_LOCALE,
  setLocale: async () => {},
});

function applyLocaleToDocument(locale: Locale): void {
  document.documentElement.lang = locale;
  document.documentElement.dir = getLocaleDirection(locale);
}

export function LocaleProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(DEFAULT_LOCALE);

  useEffect(() => {
    let cancelled = false;
    getSettings()
      .then((settings) => {
        if (cancelled) return;
        const saved = settings.locale;
        if (isSupportedLocale(saved)) setLocaleState(saved);
      })
      .catch(() => {
        // A missing backend (plain-browser preview) just keeps English.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    applyLocaleToDocument(locale);
  }, [locale]);

  const setLocale = useCallback(async (next: Locale) => {
    setLocaleState(next);
    applyLocaleToDocument(next);
    await setSetting("ui.locale", next);
  }, []);

  const value = useMemo(() => ({ locale, setLocale }), [locale, setLocale]);
  return (
    <LocaleContext.Provider value={value}>{children}</LocaleContext.Provider>
  );
}

export function useLocale(): LocaleContextValue {
  return useContext(LocaleContext);
}

/** Translator for one domain in the current locale. */
export function useT(domain: DesktopDomain): Translator {
  const { locale } = useLocale();
  return useMemo(
    () => getTranslator(locale, "desktop", domain),
    [locale, domain],
  );
}

/** Map a web/command error code to the desktop `errors.json` key. */
export function errorMessageKey(code: string): string {
  switch (code) {
    case "invalid_credentials":
      return "loginFailed";
    case "email_not_verified":
      return "emailNotVerified";
    case "account_switch_blocked":
      return "accountSwitchBlocked";
    case "invalid_2fa_code":
      return "invalid2faCode";
    case "invalid_token":
      return "invalidToken";
    case "oauth_no_account":
      return "accountNotFound";
    case "email_taken":
      return "emailTaken";
    case "oauth_failed":
      return "oauthFailed";
    case "not_configured":
      return "notConfigured";
    case "unauthorized":
      return "sessionExpired";
    case "session_expired":
      return "sessionExpired";
    case "WebUnreachable":
      return "webUnreachable";
    case "InternalError":
      return "internalError";
    case "internal_error":
      return "internalError";
    case "InvalidInput":
      return "invalidBody";
    case "invalid_body":
      return "invalidBody";
    case "registration_rate_limited":
      return "rateLimitedRegister";
    default:
      return "internalError";
  }
}

/** The error a Tauri command rejected with: its code plus the
 *  retry time, if the web sent one. */
export function commandErrorParts(error: unknown): {
  code: string | null;
  retryAt: string | null;
} {
  if (typeof error === "object" && error !== null) {
    const record = error as {
      code?: unknown;
      retryAt?: unknown;
      message?: unknown;
    };
    return {
      code: typeof record.code === "string" ? record.code : null,
      retryAt: typeof record.retryAt === "string" ? record.retryAt : null,
    };
  }
  return { code: null, retryAt: null };
}

/** Render a command error with the errors-domain translator:
 *  the localized message for the code, with `{retryAt}` replaced
 *  by the formatted retry time when the web sent one. Falls back
 *  to the backend's English message when the code is unknown. */
export function commandErrorMessage(
  error: unknown,
  tErrors: Translator,
  locale: Locale,
): string {
  const { code, retryAt } = commandErrorParts(error);
  if (code === null) {
    return error instanceof Error ? error.message : String(error);
  }
  let message = tErrors(errorMessageKey(code));
  if (retryAt !== null) {
    message = message.replace(
      "{retryAt}",
      new Intl.DateTimeFormat(locale, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(retryAt)),
    );
  }
  return message;
}
