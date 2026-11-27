import { getErrorStrings, getRequestLocale, getWebTranslator } from "@/lib/i18n";
import { ResetRequestForm } from "@/components/auth/ResetRequestForm";

export default async function ResetPage() {
  const locale = await getRequestLocale();
  const tAuth = await getWebTranslator("auth");
  const strings = {
    email: tAuth("email"),
    resetButton: tAuth("resetButton"),
    resetSent: tAuth("resetSent"),
  };
  return (
    <main>
      <h1>{tAuth("resetTitle")}</h1>
      <p>{tAuth("resetSubtitle")}</p>
      <ResetRequestForm strings={strings} errors={await getErrorStrings()} locale={locale} />
    </main>
  );
}
