import Link from "next/link";
import { getWebTranslator } from "@/lib/i18n";

export default async function HomePage() {
  const t = await getWebTranslator("common");
  return (
    <main>
      <h1>{t("appName")}</h1>
      <p>{t("tagline")}</p>
      <p>
        <Link href="/login">{t("login")}</Link>
      </p>
    </main>
  );
}
