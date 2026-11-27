export type Locale = "en" | "ur";
export type LocaleDirection = "ltr" | "rtl";
export type LocaleStatus = "complete" | "coming-soon";

export interface LocaleEntry {
  code: Locale;
  name: string;
  nativeName: string;
  dir: LocaleDirection;
  status: LocaleStatus;
}

export const SUPPORTED_LOCALES: readonly LocaleEntry[] = [
  {
    code: "en",
    name: "English",
    nativeName: "English",
    dir: "ltr",
    status: "complete",
  },
  {
    code: "ur",
    name: "Urdu",
    nativeName: "اردو",
    dir: "rtl",
    status: "complete",
  },
];

export const DEFAULT_LOCALE: Locale = "en";

/** Only complete locales are selectable in the language switcher. */
export const SELECTABLE_LOCALES: readonly LocaleEntry[] =
  SUPPORTED_LOCALES.filter((locale) => locale.status === "complete");

export function isSupportedLocale(value: unknown): value is Locale {
  return (
    typeof value === "string" &&
    SUPPORTED_LOCALES.some((locale) => locale.code === value)
  );
}

export function getLocaleDirection(code: Locale): LocaleDirection {
  return SUPPORTED_LOCALES.find((locale) => locale.code === code)?.dir ?? "ltr";
}

/** Browser locale → our locale: Urdu if the primary subtag is ur, else English. */
export function detectLocaleFromBrowserLanguage(
  language: string | undefined
): Locale {
  return language?.toLowerCase().startsWith("ur") ? "ur" : "en";
}
