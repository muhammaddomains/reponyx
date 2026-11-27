import Link from "next/link";
import type { ReactNode } from "react";
import { getWebTranslator } from "@/lib/i18n";
import { LogoutButton } from "@/components/auth/LogoutButton";

export default async function DashboardLayout({ children }: { children: ReactNode }) {
  const tCommon = await getWebTranslator("common");
  return (
    <div>
      <header>
        <Link href="/dashboard">{tCommon("appName")}</Link>
        <LogoutButton label={tCommon("logout")} />
      </header>
      {children}
    </div>
  );
}
