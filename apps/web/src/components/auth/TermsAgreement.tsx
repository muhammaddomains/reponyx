import Link from "next/link";
import { getWebTranslator } from "@/lib/i18n";
import { link } from "@/components/ui";

/**
 * The sendcms-style agreement line under the auth forms: "By signing
 * in you agree to our Terms of Service." with the terms as a link.
 * The {terms} placeholder in the string is replaced by the link.
 */
export async function TermsAgreement({ kind }: { kind: "signin" | "signup" }) {
  const tAuth = await getWebTranslator("auth");
  const tCommon = await getWebTranslator("common");
  const text = tAuth(
    kind === "signin" ? "termsAgreementSignIn" : "termsAgreementSignUp"
  );
  const [before, after] = text.split("{terms}");
  return (
    <p className="text-center text-sm text-muted-foreground">
      {before}
      <Link href="/terms" className={link}>
        {tCommon("termsOfService")}
      </Link>
      {after ?? ""}
    </p>
  );
}
