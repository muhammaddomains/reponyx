import { getErrorStrings, getRequestLocale, getWebTranslator } from "@/lib/i18n";
import { VerifyClient } from "@/components/auth/VerifyClient";

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
  };
  return (
    <main>
      <h1>{tAuth("verifyTitle")}</h1>
      {token ? (
        <VerifyClient token={token} strings={strings} errors={await getErrorStrings()} locale={locale} />
      ) : (
        <p>{tAuth("verifyFailed")}</p>
      )}
    </main>
  );
}
