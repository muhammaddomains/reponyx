import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { AuthShell } from "@/components/auth/AuthShell";
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
    <AuthShell title={tAuth("resetTitle")} description={tAuth("resetSubtitle")}>
      <ResetRequestForm
        strings={strings}
        errors={await getErrorStrings()}
        locale={locale}
      />
    </AuthShell>
  );
}
