import { NextResponse } from "next/server";
import { ApiError, errorResponse } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";
import {
  getAppInstallation,
  getGitHubAppConfig,
  githubErrorResponse,
} from "@/lib/github-app";

/**
 * Verify one installation for the local broker's install callback
 * (W4): the broker asks the web whether an installation id belongs
 * to the shared App and what its account login is, instead of
 * verifying with a local private key. 404 when the App has no such
 * installation. Session-authenticated — the account login is public
 * information, but the endpoint is not an unauthenticated oracle.
 */
export async function GET(
  req: Request,
  props: { params: Promise<{ id: string }> }
) {
  try {
    const { id } = await props.params;
    if (!/^\d+$/.test(id)) throw new ApiError("invalid_body", 400);
    const installationId = Number(id);
    const client = getDb();
    await requireSession(client, req);
    const config = getGitHubAppConfig();
    if (!config) throw new ApiError("not_configured", 503);
    let appInstallation;
    try {
      appInstallation = await getAppInstallation(config, installationId);
    } catch (err) {
      throw githubErrorResponse(err);
    }
    if (!appInstallation) {
      throw new ApiError("installation_not_linked", 404);
    }
    return NextResponse.json({
      installationId: appInstallation.id,
      accountId: appInstallation.account.id,
      accountLogin: appInstallation.account.login,
    });
  } catch (err) {
    return errorResponse(err);
  }
}
