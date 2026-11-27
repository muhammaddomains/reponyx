import { redirect } from "next/navigation";

/**
 * The old /auth/* routes moved to clean top-level URLs (/login,
 * /register, /reset, /verify). These redirect pages keep old links
 * working — including the ?token= links already sent by email — by
 * forwarding the whole query string.
 */
export async function redirectWithQuery(
  target: string,
  searchParams: Promise<Record<string, string | string[] | undefined>>
): Promise<never> {
  const params = await searchParams;
  const qs = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined) continue;
    if (Array.isArray(value)) value.forEach((v) => qs.append(key, v));
    else qs.append(key, value);
  }
  const suffix = qs.toString();
  redirect(suffix ? `${target}?${suffix}` : target);
}
