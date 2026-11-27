import Link from "next/link";
import { getWebTranslator } from "@/lib/i18n";
import { Logo } from "@/components/Logo";
import { link } from "@/components/ui";

const SECTIONS: { heading: string; body: string[] }[] = [
  {
    heading: "1. The service",
    body: [
      "RepoNyx is a desktop application that syncs and archives Git repositories from your GitHub accounts onto your own machine, plus a web control plane at reponyx.bond where you manage your account, linked GitHub accounts, devices, and package.",
      "The web never sees your repository contents, your personal access tokens, or your GitHub tokens. Clone authentication stays on your own machine: the desktop app, its background agent, and the local auth broker.",
    ],
  },
  {
    heading: "2. Accounts",
    body: [
      "You must register on the web to use RepoNyx. Registration is web-only; the desktop app only logs in. You are responsible for keeping your credentials secret and for everything done under your account.",
      "You may register with an email and password, or with Google or GitHub on the register page. A Google or GitHub account that is already linked to a RepoNyx account cannot register a second one — use the login page instead.",
      "One registration per IP address every 24 hours. After logout, the same account can log back in immediately; a different account from the same device must wait one hour.",
    ],
  },
  {
    heading: "3. Packages and limits",
    body: [
      "Packages are created and priced by the administrator. The Free package allows two linked GitHub accounts and two devices. Paid packages raise those limits as shown on the package at the time of purchase.",
      "When a paid package expires, your limits return to the Free package. Linked accounts beyond the limit pause syncing — nothing is deleted and your local repositories stay untouched — until you upgrade or unlink accounts.",
    ],
  },
  {
    heading: "4. Acceptable use",
    body: [
      "Use RepoNyx only for repositories you are allowed to access. Do not use the service to abuse GitHub, to circumvent GitHub's terms or rate limits, to harass others, or to break the law.",
      "We may suspend or close accounts that abuse the service, attempt to access other users' data, or interfere with the service for others.",
    ],
  },
  {
    heading: "5. Your data",
    body: [
      "We store your email address, password hash, name, linked GitHub account logins, devices, package, and subscription state. We never store your GitHub tokens or personal access tokens on the web — those live only in your operating system's credential store.",
      "The GitHub App private key is held only in the web server's environment so it can mint installation tokens for your own installations. It is never shipped in the desktop app, never stored in the database, and never logged.",
      "We keep an audit log of account actions (registration, login, device changes) for security. We do not sell your data.",
    ],
  },
  {
    heading: "6. GitHub access",
    body: [
      "RepoNyx connects to GitHub through the GitHub App you install on your GitHub accounts, or through a fine-grained personal access token you provide. GitHub's own terms apply to that access, and GitHub may change or revoke it at any time.",
      "Syncing copies repositories to your machine. You are responsible for having the right to copy them and for keeping your machine secure.",
    ],
  },
  {
    heading: "7. Availability and liability",
    body: [
      "The service is provided as-is, without warranties of any kind. We aim for reliable service but do not guarantee uninterrupted availability, and we are not liable for lost data, missed syncs, or downtime.",
      "Your local repositories are your responsibility — keep your own backups. RepoNyx is an archive tool, not a backup guarantee.",
    ],
  },
  {
    heading: "8. Termination",
    body: [
      "You may stop using RepoNyx at any time and request deletion of your web account data. We may suspend or close accounts that violate these terms. Closing your web account does not delete repositories already on your machine.",
    ],
  },
  {
    heading: "9. Changes",
    body: [
      "We may update these terms. The current version is always shown on this page with its last-updated date. Continuing to use RepoNyx after a change means you accept the updated terms.",
    ],
  },
  {
    heading: "10. Contact",
    body: [
      "Questions about these terms or your account: use the contact form on the website, or write to the support address shown there.",
    ],
  },
];

export default async function TermsPage() {
  const tCommon = await getWebTranslator("common");
  return (
    <main className="mx-auto w-full max-w-3xl px-6 py-12">
      <header className="mb-8 space-y-4">
        <Logo />
        <h1 className="text-3xl font-bold tracking-tight">
          {tCommon("termsOfService")}
        </h1>
        <p className="text-sm text-muted-foreground">
          Last updated: November 27, 2026
        </p>
      </header>
      <div className="space-y-8">
        {SECTIONS.map((section) => (
          <section key={section.heading}>
            <h2 className="mb-2 text-xl font-semibold">{section.heading}</h2>
            {section.body.map((paragraph) => (
              <p key={paragraph.slice(0, 32)} className="mb-3 leading-7">
                {paragraph}
              </p>
            ))}
          </section>
        ))}
      </div>
      <p className="mt-10">
        <Link href="/login" className={link}>
          {tCommon("back")} — {tCommon("login")}
        </Link>
      </p>
    </main>
  );
}
