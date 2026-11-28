import Link from "next/link";
import { getWebTranslator } from "@/lib/i18n";
import { outlineButton, primaryButton } from "@/components/ui";

/** Shown after a successful checkout (W6). */
export default async function BillingSuccessPage() {
  const t = await getWebTranslator("billing");
  return (
    <main
      id="main-content"
      className="flex min-h-svh flex-col items-center justify-center px-4 py-16 text-center"
    >
      <h1 className="mb-3 text-2xl font-bold tracking-tight">
        {t("successTitle")}
      </h1>
      <p className="mb-8 max-w-md leading-relaxed text-muted-foreground">
        {t("successBody")}
      </p>
      <Link href="/dashboard" className={`${primaryButton} w-auto px-6`}>
        {t("backToDashboard")}
      </Link>
    </main>
  );
}
