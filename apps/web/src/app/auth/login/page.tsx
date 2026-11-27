import Link from "next/link";
import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { AuthShell } from "@/components/auth/AuthShell";
import { LoginForm } from "@/components/auth/LoginForm";
import { SocialSignInButtons } from "@/components/auth/SocialSignInButtons";
import { alertError, link } from "@/components/ui";

const OAUTH_ERROR_KEYS: Record<string, string> = {
  oauth_no_account: "accountNotFound",
  oauth_failed: "oauthFailed",
  oauth_not_configured: "notConfigured",
  two_factor_required: "twoFactorRequired",
  account_switch_blocked: "accountSwitchBlocked",
};

export default async function LoginPage(props: {
  searchParams: Promise<{ error?: string; retry_at?: string }>;
}) {
  const { error, retry_at: retryAt } = await props.searchParams;
  const locale = await getRequestLocale();
  const tAuth = await getWebTranslator("auth");
  const errors = await getErrorStrings();
  const strings = {
    email: tAuth("email"),
    password: tAuth("password"),
    loginButton: tAuth("loginButton"),
  };
  let oauthError =
    error && OAUTH_ERROR_KEYS[error] ? errors[OAUTH_ERROR_KEYS[error]] : null;
  if (oauthError && retryAt) {
    oauthError = oauthError.replace(
      "{retryAt}",
      new Intl.DateTimeFormat(locale, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(retryAt))
    );
  }
  return (
    <AuthShell title={tAuth("loginTitle")} description={tAuth("loginSubtitle")}>
      {oauthError ? (
        <p role="alert" className={alertError}>
          {oauthError}
        </p>
      ) : null}
      <LoginForm strings={strings} errors={errors} locale={locale} />
      <SocialSignInButtons
        googleLabel={tAuth("continueWithGoogle")}
        githubLabel={tAuth("continueWithGitHub")}
        dividerLabel={tAuth("orContinueWith")}
        googleEnabled={Boolean(process.env.GOOGLE_CLIENT_ID)}
        githubEnabled={Boolean(process.env.GITHUB_OAUTH_CLIENT_ID)}
      />
      <div className="flex items-center justify-between">
        <Link href="/auth/reset" className={link}>
          {tAuth("forgotPassword")}
        </Link>
        <Link href="/auth/register" className={link}>
          {tAuth("noAccount")}
        </Link>
      </div>
    </AuthShell>
  );
}
