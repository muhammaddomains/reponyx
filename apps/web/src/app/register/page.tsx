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
import { RegisterForm } from "@/components/auth/RegisterForm";
import { SocialSignInButtons } from "@/components/auth/SocialSignInButtons";
import { TermsAgreement } from "@/components/auth/TermsAgreement";
import { alertError, link } from "@/components/ui";

const REGISTER_ERROR_KEYS: Record<string, string> = {
  email_taken: "emailTaken",
  registration_rate_limited: "rateLimitedRegister",
  oauth_failed: "oauthFailed",
  oauth_not_configured: "notConfigured",
};

export default async function RegisterPage(props: {
  searchParams: Promise<{ error?: string; retry_at?: string }>;
}) {
  // Already signed in — the register form is not the page for you.
  const session = await getSessionFromCookies(getDb());
  if (session) redirect("/dashboard");
  const { error, retry_at: retryAt } = await props.searchParams;
  const locale = await getRequestLocale();
  const tAuth = await getWebTranslator("auth");
  const errors = await getErrorStrings();
  const strings = {
    fullName: tAuth("fullName"),
    email: tAuth("email"),
    password: tAuth("password"),
    registerButton: tAuth("registerButton"),
    registerSuccess: tAuth("registerSuccess"),
  };
  let registerError =
    error && REGISTER_ERROR_KEYS[error]
      ? errors[REGISTER_ERROR_KEYS[error]]
      : null;
  if (registerError && retryAt) {
    registerError = registerError.replace(
      "{retryAt}",
      new Intl.DateTimeFormat(locale, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(retryAt))
    );
  }
  return (
    <AuthShell
      title={tAuth("registerTitle")}
      description={tAuth("registerSubtitle")}
    >
      {registerError ? (
        <p role="alert" className={alertError}>
          {registerError}
        </p>
      ) : null}
      <SocialSignInButtons
        googleLabel={tAuth("continueWithGoogle")}
        githubLabel={tAuth("continueWithGitHub")}
        dividerLabel={tAuth("orContinueWithEmail")}
        googleEnabled={Boolean(process.env.GOOGLE_CLIENT_ID)}
        githubEnabled={Boolean(process.env.GITHUB_OAUTH_CLIENT_ID)}
        intent="register"
      />
      <RegisterForm strings={strings} errors={errors} locale={locale} />
      <p className="text-center">
        <Link href="/login" className={link}>
          {tAuth("alreadyHaveAccount")}
        </Link>
      </p>
      <TermsAgreement kind="signup" />
    </AuthShell>
  );
}
