import { hashToken } from "@reponyx/auth";
import { findValidResetToken } from "@reponyx/database";
import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { getDb } from "@/lib/db";
import { AuthShell } from "@/components/auth/AuthShell";
import { ResetConfirmForm } from "@/components/auth/ResetConfirmForm";
import { alertError } from "@/components/ui";

export default async function ResetConfirmPage(props: {
  searchParams: Promise<{ token?: string }>;
}) {
  const { token } = await props.searchParams;
  const locale = await getRequestLocale();
  const tAuth = await getWebTranslator("auth");
  const tErrors = await getWebTranslator("errors");
  const strings = {
    newPassword: tAuth("newPassword"),
    confirmPassword: tAuth("confirmPassword"),
    passwordsDoNotMatch: tAuth("passwordsDoNotMatch"),
    changePasswordButton: tAuth("changePasswordButton"),
    resetSuccess: tAuth("resetSuccess"),
  };
  // A link that was already used (or expired) shows the dead-link
  // message instead of the form — one link resets the password
  // exactly once.
  const tokenValid = token
    ? (await findValidResetToken(getDb(), hashToken(token))) !== null
    : false;
  return (
    <AuthShell
      title={tAuth("resetTitle")}
      description={tAuth("confirmResetSubtitle")}
    >
      {tokenValid && token ? (
        <ResetConfirmForm
          token={token}
          strings={strings}
          errors={await getErrorStrings()}
          locale={locale}
        />
      ) : (
        <p role="alert" className={alertError}>
          {tErrors("invalidToken")}
        </p>
      )}
    </AuthShell>
  );
}
