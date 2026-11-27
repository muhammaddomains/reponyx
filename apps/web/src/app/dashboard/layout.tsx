import Link from "next/link";
import type { ReactNode } from "react";
import { Logo } from "@/components/Logo";
import { LogoutButton } from "@/components/auth/LogoutButton";
import { getWebTranslator } from "@/lib/i18n";

export default async function DashboardLayout({
  children,
}: {
  children: ReactNode;
}) {
  const tCommon = await getWebTranslator("common");
  return (
    <div className="min-h-svh bg-background text-foreground">
      <header className="sticky top-0 z-40 border-b border-border bg-background/95 backdrop-blur">
        <div className="mx-auto flex h-14 w-full max-w-5xl items-center justify-between px-6">
          <Link href="/dashboard" className="flex items-center">
            <Logo size={28} showWordmark />
          </Link>
          <LogoutButton label={tCommon("logout")} />
        </div>
      </header>
      {children}
    </div>
  );
}
