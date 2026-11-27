import Link from "next/link";
import {
  getErrorStrings,
  getRequestLocale,
  getWebTranslator,
} from "@/lib/i18n";
import { AuthShell } from "@/components/auth/AuthShell";
import { RegisterForm } from "@/components/auth/RegisterForm";
import { link } from "@/components/ui";

export default async function RegisterPage() {
  const locale = await getRequestLocale();
  const tAuth = await getWebTranslator("auth");
  const strings = {
    email: tAuth("email"),
    password: tAuth("password"),
    registerButton: tAuth("registerButton"),
    registerSuccess: tAuth("registerSuccess"),
  };
  return (
    <AuthShell
      title={tAuth("registerTitle")}
      description={tAuth("registerSubtitle")}
    >
      <RegisterForm
        strings={strings}
        errors={await getErrorStrings()}
        locale={locale}
      />
      <p className="text-center">
        <Link href="/auth/login" className={link}>
          {tAuth("alreadyHaveAccount")}
        </Link>
      </p>
    </AuthShell>
  );
}
