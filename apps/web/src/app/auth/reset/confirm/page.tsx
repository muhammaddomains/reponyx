import { getErrorStrings, getRequestLocale, getWebTranslator } from "@/lib/i18n";
import { ResetConfirmForm } from "@/components/auth/ResetConfirmForm";

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
    <main>
      <h1>{tAuth("resetTitle")}</h1>
      {token ? (
        <ResetConfirmForm token={token} strings={strings} errors={await getErrorStrings()} locale={locale} />
      ) : (
        <p>{tErrors("invalidToken")}</p>
      )}
    </main>
  );
}
