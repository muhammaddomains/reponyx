import { NextResponse } from "next/server";
import {
  buildAuthorizationUrl,
  createOAuthState,
  getOAuthProviderConfig,
} from "@/lib/oauth";

export async function GET(req: Request) {
  const url = new URL(req.url);
  const intentParam = url.searchParams.get("intent");
  const intent =
    intentParam === "register" || intentParam === "link"
      ? intentParam
      : "login";
  const config = getOAuthProviderConfig("google");
  const state = config ? createOAuthState("google", intent) : null;
  if (!config || !state) {
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
