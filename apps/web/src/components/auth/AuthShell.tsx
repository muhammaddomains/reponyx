import Link from "next/link";
import type { ReactNode } from "react";
import { Logo } from "@/components/Logo";
import { getWebTranslator } from "@/lib/i18n";

/**
 * Shared two-column layout for the auth pages (sendcms AuthShell pattern):
 * brand panel on the left, form on the right.
 */
export async function AuthShell({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  const tCommon = await getWebTranslator("common");
  return (
    <div className="grid min-h-svh lg:grid-cols-2">
      {/* Left — brand panel */}
      <div className="hidden flex-col justify-between border-e border-border bg-muted px-10 py-12 lg:flex">
        <Link href="/" className="flex items-center">
          <Logo size={40} showWordmark />
        </Link>
        <div>
          <p className="text-2xl font-semibold tracking-tight text-pretty">
            {tCommon("tagline")}
          </p>
          <p className="mt-2 max-w-md leading-relaxed text-muted-foreground">
            {tCommon("brandBlurb")}
          </p>
        </div>
        <p className="text-sm text-muted-foreground">{tCommon("appName")}</p>
      </div>
      {/* Right — form */}
      <div className="flex flex-col items-center justify-center px-6 py-16">
        {/* Mobile logo */}
        <Link href="/" className="mb-10 flex items-center lg:hidden">
          <Logo size={40} showWordmark />
        </Link>
        <div className="w-full max-w-90 space-y-8">
          <div className="space-y-1.5">
            <h1 className="text-2xl font-semibold tracking-tight text-pretty">
              {title}
            </h1>
            <p className="text-sm text-muted-foreground">{description}</p>
          </div>
          {children}
        </div>
      </div>
    </div>
  );
}
