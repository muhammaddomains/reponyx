/** Locale-aware formatting helpers (Intl only — no date/number library). */

function safeLocale(locale: string): string {
  try {
    // Throws RangeError for an unknown locale; fall back to English.
    new Intl.DateTimeFormat(locale);
    return locale;
  } catch {
    return "en";
  }
}

export function formatDate(locale: string, date: Date): string {
  return new Intl.DateTimeFormat(safeLocale(locale)).format(date);
}

export function formatTime(locale: string, date: Date): string {
  return new Intl.DateTimeFormat(safeLocale(locale), {
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function formatNumber(locale: string, value: number): string {
  return new Intl.NumberFormat(safeLocale(locale)).format(value);
}

export function formatCurrency(locale: string, value: number, currency: string): string {
  return new Intl.NumberFormat(safeLocale(locale), {
    style: "currency",
    currency,
  }).format(value);
}

export function formatPercent(locale: string, value: number): string {
  return new Intl.NumberFormat(safeLocale(locale), { style: "percent" }).format(value);
}

export function formatRelativeTime(
  locale: string,
  value: number,
  unit: Intl.RelativeTimeFormatUnit,
): string {
  return new Intl.RelativeTimeFormat(safeLocale(locale), { numeric: "auto" }).format(
    value,
    unit,
  );
}

export function formatRelativeDate(locale: string, date: Date, now = new Date()): string {
  const diffMs = date.getTime() - now.getTime();
  const diffDays = Math.round(diffMs / 86_400_000);
  if (Math.abs(diffDays) >= 1) return formatRelativeTime(locale, diffDays, "day");
  const diffHours = Math.round(diffMs / 3_600_000);
  if (Math.abs(diffHours) >= 1) return formatRelativeTime(locale, diffHours, "hour");
  const diffMinutes = Math.round(diffMs / 60_000);
  return formatRelativeTime(locale, diffMinutes, "minute");
}
