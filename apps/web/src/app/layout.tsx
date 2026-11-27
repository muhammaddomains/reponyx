import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";
import { Geist, Geist_Mono } from "next/font/google";
import { cookies } from "next/headers";
import { getLocaleDirection, isSupportedLocale } from "@reponyx/i18n";
import { ThemeProvider } from "@/components/ThemeProvider";
import "./globals.css";

const fontSans = Geist({
  subsets: ["latin"],
  variable: "--font-sans",
});

const fontMono = Geist_Mono({
  subsets: ["latin"],
  variable: "--font-mono",
});

const appUrl = process.env.NEXT_PUBLIC_APP_URL ?? "https://reponyx.bond";

export const metadata: Metadata = {
  title: "RepoNyx",
  description: "Your Repositories. One Local Universe",
  metadataBase: new URL(appUrl),
  // Authenticated app — nothing here is for crawlers.
  robots: { index: false, follow: false },
  manifest: "/site.webmanifest",
  icons: {
    icon: [
      { url: "/favicon.ico" },
      { url: "/favicon-32x32.png", sizes: "32x32", type: "image/png" },
      { url: "/favicon-16x16.png", sizes: "16x16", type: "image/png" },
    ],
    apple: [{ url: "/apple-touch-icon.png", sizes: "180x180" }],
  },
  openGraph: {
    title: "RepoNyx",
    description: "Your Repositories. One Local Universe",
    url: appUrl,
    siteName: "RepoNyx",
    images: [
      {
        url: "/og-image.png",
        width: 1200,
        height: 630,
        alt: "RepoNyx — Your Repositories. One Local Universe",
      },
    ],
    type: "website",
  },
  twitter: {
    card: "summary_large_image",
    title: "RepoNyx",
    description: "Your Repositories. One Local Universe",
    images: ["/og-image.png"],
  },
};

// Browser-chrome theme colour, light and dark (sendcms dashboard pattern).
export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#0ea472" },
    { media: "(prefers-color-scheme: dark)", color: "#12d393" },
  ],
};

export default async function RootLayout({
  children,
}: {
  children: ReactNode;
}) {
  const cookieStore = await cookies();
  const raw = cookieStore.get("reponyx-locale")?.value;
  const locale = isSupportedLocale(raw) ? raw : "en";
  return (
    <html
      lang={locale}
      dir={getLocaleDirection(locale)}
      suppressHydrationWarning
      className={`${fontSans.variable} ${fontMono.variable} font-sans antialiased`}
    >
      <body suppressHydrationWarning>
        <a
          href="#main-content"
          className="sr-only focus:not-sr-only focus:fixed focus:start-4 focus:top-4 focus:z-50 focus:rounded focus:bg-background focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:shadow"
        >
          Skip to content
        </a>
        <ThemeProvider>{children}</ThemeProvider>
      </body>
    </html>
  );
}
