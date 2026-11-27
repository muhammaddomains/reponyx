import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
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
    resetButton: tAuth("resetButton"),
    resetSuccess: tAuth("resetSuccess"),
  };
  return (
    <AuthShell title={tAuth("resetTitle")} description={tAuth("resetSubtitle")}>
      {token ? (
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
