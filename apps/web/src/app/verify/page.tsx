import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { AuthShell } from "@/components/auth/AuthShell";
import { VerifyClient } from "@/components/auth/VerifyClient";
import { alertError } from "@/components/ui";

export default async function VerifyPage(props: {
  searchParams: Promise<{ token?: string }>;
}) {
  const { token } = await props.searchParams;
  const locale = await getRequestLocale();
  const tAuth = await getWebTranslator("auth");
  const tCommon = await getWebTranslator("common");
  const strings = {
    loading: tCommon("loading"),
    verifySuccess: tAuth("verifySuccess"),
    verifyFailed: tAuth("verifyFailed"),
    loginButton: tAuth("loginButton"),
  };
  return (
    <AuthShell
      title={tAuth("verifyTitle")}
      description={tAuth("verifySubtitle")}
    >
      {token ? (
        <VerifyClient
          token={token}
          strings={strings}
          errors={await getErrorStrings()}
          locale={locale}
        />
      ) : (
        <p role="alert" className={alertError}>
          {tAuth("verifyFailed")}
        </p>
      )}
    </AuthShell>
  );
}
