import Link from "next/link";
import { getErrorStrings, getRequestLocale, getWebTranslator } from "@/lib/i18n";
import { LoginForm } from "@/components/auth/LoginForm";

export default async function LoginPage() {
  const locale = await getRequestLocale();
  const tAuth = await getWebTranslator("auth");
  const strings = {
    email: tAuth("email"),
    password: tAuth("password"),
    loginButton: tAuth("loginButton"),
  };
  return (
    <main>
      <h1>{tAuth("loginTitle")}</h1>
      <p>{tAuth("loginSubtitle")}</p>
      <LoginForm strings={strings} errors={await getErrorStrings()} locale={locale} />
      <p>
        <Link href="/auth/reset">{tAuth("forgotPassword")}</Link>
      </p>
      <p>
        <Link href="/auth/register">{tAuth("noAccount")}</Link>
      </p>
    </main>
  );
}
