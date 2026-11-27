import { cookies } from "next/headers";
import {
  DEFAULT_LOCALE,
  getTranslator as getPackageTranslator,
  isSupportedLocale,
  registerTranslations,
  type Locale,
  type WebDomain,
} from "@reponyx/i18n";
import commonEn from "@reponyx/i18n/locales/en/web/common.json";
import authEn from "@reponyx/i18n/locales/en/web/auth.json";
import dashboardEn from "@reponyx/i18n/locales/en/web/dashboard.json";
import errorsEn from "@reponyx/i18n/locales/en/web/errors.json";
import commonUr from "@reponyx/i18n/locales/ur/web/common.json";
import authUr from "@reponyx/i18n/locales/ur/web/auth.json";
import dashboardUr from "@reponyx/i18n/locales/ur/web/dashboard.json";
import errorsUr from "@reponyx/i18n/locales/ur/web/errors.json";

// Register once at module load; reads are then synchronous.
// (Same pattern as sendcms's @sendcms/i18n: static JSON import +
// registerTranslations, never Node fs at runtime.)
registerTranslations("en", "web", "common", commonEn);
registerTranslations("en", "web", "auth", authEn);
registerTranslations("en", "web", "dashboard", dashboardEn);
registerTranslations("en", "web", "errors", errorsEn);
registerTranslations("ur", "web", "common", commonUr);
registerTranslations("ur", "web", "auth", authUr);
registerTranslations("ur", "web", "dashboard", dashboardUr);
registerTranslations("ur", "web", "errors", errorsUr);

export async function getRequestLocale(): Promise<Locale> {
  const cookieStore = await cookies();
  const raw = cookieStore.get("reponyx-locale")?.value;
  return isSupportedLocale(raw) ? raw : DEFAULT_LOCALE;
}

export async function getWebTranslator(domain: WebDomain) {
  const locale = await getRequestLocale();
  return getPackageTranslator(locale, "web", domain);
}

/** Every error code the API can return, localized for client forms. */
export async function getErrorStrings(): Promise<Record<string, string>> {
  const t = await getWebTranslator("errors");
  return {
    loginFailed: t("loginFailed"),
    emailNotVerified: t("emailNotVerified"),
    emailTaken: t("emailTaken"),
    rateLimitedRegister: t("rateLimitedRegister"),
    accountSwitchBlocked: t("accountSwitchBlocked"),
    invalidToken: t("invalidToken"),
    invalidBody: t("invalidBody"),
    emailUnavailable: t("emailUnavailable"),
    internalError: t("internalError"),
    sessionExpired: t("sessionExpired"),
    notConfigured: t("notConfigured"),
    oauthFailed: t("oauthFailed"),
    twoFactorRequired: t("twoFactorRequired"),
  };
}
