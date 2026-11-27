import { NextResponse } from "next/server";
import {
  buildAuthorizationUrl,
  createOAuthState,
  getOAuthProviderConfig,
} from "@/lib/oauth";

export async function GET(req: Request) {
  const config = getOAuthProviderConfig("github");
  const state = config ? createOAuthState("github") : null;
  if (!config || !state) {
    return NextResponse.redirect(
      new URL("/auth/login?error=oauth_not_configured", req.url)
    );
  }
  return NextResponse.redirect(buildAuthorizationUrl(config, state));
}
