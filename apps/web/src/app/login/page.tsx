import { redirect } from "next/navigation";
import Link from "next/link";
import { getSessionFromCookies } from "@/lib/auth";
import { getDb } from "@/lib/db";
import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { AuthShell } from "@/components/auth/AuthShell";
import { LoginForm } from "@/components/auth/LoginForm";
import { SocialSignInButtons } from "@/components/auth/SocialSignInButtons";
import { TermsAgreement } from "@/components/auth/TermsAgreement";
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
  // Already signed in — the login form is not the page for you.
  const session = await getSessionFromCookies(getDb());
  if (session) redirect("/dashboard");
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
      <SocialSignInButtons
        googleLabel={tAuth("continueWithGoogle")}
        githubLabel={tAuth("continueWithGitHub")}
        dividerLabel={tAuth("orContinueWithEmail")}
        googleEnabled={Boolean(process.env.GOOGLE_CLIENT_ID)}
        githubEnabled={Boolean(process.env.GITHUB_OAUTH_CLIENT_ID)}
      />
      <LoginForm strings={strings} errors={errors} locale={locale} />
      <div className="flex items-center justify-between">
        <Link href="/reset" className={link}>
          {tAuth("forgotPassword")}
        </Link>
        <Link href="/register" className={link}>
          {tAuth("noAccount")}
        </Link>
      </div>
      <TermsAgreement kind="signin" />
    </AuthShell>
  );
}
