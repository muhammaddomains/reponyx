export type EmailProvider = "resend" | "smtp" | "none";

export interface EmailEnv {
  EMAIL_PROVIDER?: string;
  RESEND_API_KEY?: string;
  RESEND_FROM?: string;
  SMTP_HOST?: string;
  SMTP_PORT?: string;
  SMTP_USER?: string;
  SMTP_PASS?: string;
  SMTP_FROM?: string;
}

/**
 * Provider selection, mirroring sendcms's `resolveEmailProvider` with one
 * difference the user asked for: an explicit selector. `EMAIL_PROVIDER`
 * picks the provider when set (`resend` | `smtp`); both credential sets
 * can live in `.env` at once. Unset → Resend if its key is present,
 * else SMTP if configured, else `none` (callers fail loudly).
 */
export function resolveEmailProvider(env: EmailEnv): EmailProvider {
  const selected = env.EMAIL_PROVIDER?.trim().toLowerCase();
  if (selected === "resend") return env.RESEND_API_KEY ? "resend" : "none";
  if (selected === "smtp") return hasSmtpConfig(env) ? "smtp" : "none";
  if (env.RESEND_API_KEY) return "resend";
  if (hasSmtpConfig(env)) return "smtp";
  return "none";
}

export function hasSmtpConfig(env: EmailEnv): boolean {
  return Boolean(env.SMTP_HOST && env.SMTP_USER && env.SMTP_PASS);
}

/** What the admin sees: whether each provider is configured — never the keys. */
export function describeEmailConfig(env: EmailEnv): {
  provider: EmailProvider;
  resendConfigured: boolean;
  smtpConfigured: boolean;
} {
  return {
    provider: resolveEmailProvider(env),
    resendConfigured: Boolean(env.RESEND_API_KEY),
    smtpConfigured: hasSmtpConfig(env),
  };
}
