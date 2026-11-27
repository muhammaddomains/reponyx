import Link from "next/link";
import { getErrorStrings, getRequestLocale, getWebTranslator } from "@/lib/i18n";
import { RegisterForm } from "@/components/auth/RegisterForm";

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
    <main>
      <h1>{tAuth("registerTitle")}</h1>
      <p>{tAuth("registerSubtitle")}</p>
      <RegisterForm strings={strings} errors={await getErrorStrings()} locale={locale} />
      <p>
        <Link href="/auth/login">{tAuth("alreadyHaveAccount")}</Link>
      </p>
    </main>
  );
}
