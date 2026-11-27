import { NextResponse } from "next/server";
import {
  buildAuthorizationUrl,
  createOAuthState,
  getOAuthProviderConfig,
} from "@/lib/oauth";

export async function GET(req: Request) {
  const url = new URL(req.url);
  const intentParam = url.searchParams.get("intent");
  const isDesktop = url.searchParams.get("client") === "desktop";
  // The desktop only logs in (registration stays on the web), so a
  // desktop start always signs a login intent.
  const intent =
    !isDesktop && (intentParam === "register" || intentParam === "link")
      ? intentParam
      : "login";
  const config = getOAuthProviderConfig("google");
  const state = config
    ? createOAuthState("google", intent, isDesktop ? "desktop" : "web")
    : null;
  if (!config || !state) {
    if (isDesktop) {
      return NextResponse.redirect(
        new URL("reponyx://auth?error=oauth_not_configured")
      );
    }
    return NextResponse.redirect(
      new URL(
        intent === "register"
          ? "/register?error=oauth_not_configured"
          : intent === "link"
            ? "/dashboard?link_error=oauth_not_configured"
            : "/login?error=oauth_not_configured",
        req.url
      )
    );
  }
  return NextResponse.redirect(buildAuthorizationUrl(config, state));
}
