"use client";

export interface ApiResult {
  ok: boolean;
  data: Record<string, unknown>;
}

export async function postJson(
  path: string,
  body: unknown
): Promise<ApiResult> {
  const res = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  return { ok: res.ok, data };
}

const CODE_TO_MESSAGE_KEY: Record<string, string> = {
  invalid_credentials: "loginFailed",
  email_not_verified: "emailNotVerified",
  email_taken: "emailTaken",
  already_linked: "alreadyLinked",
  email_mismatch: "emailMismatch",
  last_login_method: "lastLoginMethod",
  registration_rate_limited: "rateLimitedRegister",
  account_switch_blocked: "accountSwitchBlocked",
  invalid_token: "invalidToken",
  invalid_body: "invalidBody",
  invalid_2fa_code: "invalid2faCode",
  email_unavailable: "emailUnavailable",
  internal_error: "internalError",
  unauthorized: "sessionExpired",
  not_configured: "notConfigured",
  installation_not_linked: "installationNotLinked",
  device_revoked: "deviceRevoked",
  limit_exceeded: "limitExceeded",
  forbidden: "forbidden",
  admin_2fa_required: "admin2faRequired",
  two_factor_already_enabled: "twoFactorAlreadyEnabled",
  no_pending_2fa: "noPending2fa",
  two_factor_not_enabled: "twoFactorNotEnabled",
  billing_not_configured: "billingNotConfigured",
  package_not_for_sale: "packageNotForSale",
  package_not_found: "packageNotFound",
  provider_not_available: "providerNotAvailable",
};

/** Renders a structured API error ({code, data}) from errors.json, with the retry time filled in. */
export function errorMessage(
  errors: Record<string, string>,
  locale: string,
  data: Record<string, unknown>
): string {
  const err = (data.error ?? {}) as {
    code?: string;
    data?: Record<string, unknown>;
  };
  const code = err.code ?? "internal_error";
  const key = CODE_TO_MESSAGE_KEY[code] ?? "internalError";
  let message =
    errors[key] ?? errors.internalError ?? "Something went wrong — try again";
  const retryAt = (err.data as { retry_at?: string } | undefined)?.retry_at;
  if (retryAt) {
    message = message.replace(
      "{retryAt}",
      new Intl.DateTimeFormat(locale, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(retryAt))
    );
  }
  return message;
}
