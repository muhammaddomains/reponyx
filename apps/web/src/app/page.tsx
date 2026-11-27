import Link from "next/link";
import { getSessionFromCookies } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { getWebTranslator } from "@/lib/i18n";
import { Logo } from "@/components/Logo";
import { primaryButton, outlineButton } from "@/components/ui";

export default async function HomePage() {
  const t = await getWebTranslator("common");
  const session = await getSessionFromCookies(getDb());
  return (
    <main
      id="main-content"
      className="flex min-h-svh flex-col items-center justify-center px-4 py-16 text-center"
    >
      <Logo size={72} className="mb-6" />
      <h1 className="mb-3 text-2xl font-bold tracking-tight">{t("appName")}</h1>
      <p className="mb-8 max-w-md leading-relaxed text-muted-foreground">
        {t("tagline")}
      </p>
      {session ? (
        <Link href="/dashboard" className={`${primaryButton} w-auto px-6`}>
          {t("dashboard")}
        </Link>
      ) : (
        <div className="flex flex-col gap-3 sm:flex-row">
          <Link href="/login" className={`${primaryButton} w-auto px-6`}>
            {t("login")}
          </Link>
          <Link href="/register" className={`${outlineButton} w-auto px-6`}>
            {t("register")}
          </Link>
        </div>
      )}
    </main>
  );
}
