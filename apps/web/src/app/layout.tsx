import type { Metadata } from "next";
import type { ReactNode } from "react";
import { cookies } from "next/headers";
import { getLocaleDirection, isSupportedLocale } from "@reponyx/i18n";

export const metadata: Metadata = {
  title: "RepoNyx",
  description: "Your Repositories. One Local Universe",
};

export default async function RootLayout({ children }: { children: ReactNode }) {
  const cookieStore = await cookies();
  const raw = cookieStore.get("reponyx-locale")?.value;
  const locale = isSupportedLocale(raw) ? raw : "en";
  return (
    <html lang={locale} dir={getLocaleDirection(locale)}>
      <body>{children}</body>
    </html>
  );
}
