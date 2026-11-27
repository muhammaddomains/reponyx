import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { AuthShell } from "@/components/auth/AuthShell";
import { CompleteRegistrationForm } from "@/components/auth/CompleteRegistrationForm";
import { verifyPendingRegistrationToken } from "@/lib/oauth";
import { alertError } from "@/components/ui";

export default async function CompleteRegistrationPage(props: {
  searchParams: Promise<{ token?: string }>;
}) {
  const { token } = await props.searchParams;
  const locale = await getRequestLocale();
  const tAuth = await getWebTranslator("auth");
  const errors = await getErrorStrings();
  const pending = token ? verifyPendingRegistrationToken(token) : null;
  const strings = {
    fullName: tAuth("fullName"),
    email: tAuth("email"),
    registerButton: tAuth("registerButton"),
  };
  return (
    <AuthShell
      title={tAuth("completeProfileTitle")}
      description={tAuth("completeProfileSubtitle")}
    >
      {!pending || !token ? (
        <p role="alert" className={alertError}>
          {errors.invalidToken}
        </p>
      ) : (
        <CompleteRegistrationForm
          token={token}
          email={pending.email}
          strings={strings}
          errors={errors}
          locale={locale}
        />
      )}
    </AuthShell>
  );
}
