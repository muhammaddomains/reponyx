import { NextResponse } from "next/server";
import { z } from "zod";
import {
  findActiveSubscriptionForUser,
  findDeviceByKey,
  findInstallationByGithubId,
  findLinkedAccountByProviderAccount,
  findPackageByName,
  linkedAccountRank,
  recordAudit,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";
import {
  getAppInstallation,
  getGitHubAppConfig,
  githubErrorResponse,
  mintInstallationToken,
} from "@/lib/github-app";

const MintBody = z.object({
  installationId: z.union([
    z.number().int().positive(),
    z.string().regex(/^\d+$/),
  ]),
  deviceIdentifier: z.string().min(1).max(128).optional(),
});

/**
 * Mint an installation token for the local broker (W4). The broker
 * calls this with the user's web session instead of signing with a
 * local private key — the key never leaves the server. Checks, in
 * order: session, installation linked to this user, installation
 * still exists under the App (a GitHub-side uninstall stops minting
 * even if the web rows linger), device not revoked, and the linked
 * account within the package limit (oldest-first: on expiry back to
 * Free, the oldest accounts keep minting and only the excess ones
 * pause). The token is returned, never logged or stored.
 */
export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, MintBody);
    const installationId =
      typeof body.installationId === "number"
        ? body.installationId
        : Number(body.installationId);
    const client = getDb();
    const { user } = await requireSession(client, req);
    const config = getGitHubAppConfig();
    if (!config) throw new ApiError("not_configured", 503);
    const installation = await findInstallationByGithubId(
      client,
      installationId
    );
    if (!installation || installation.user_id !== user.id) {
      throw new ApiError("installation_not_linked", 404);
    }
    let appInstallation;
    try {
      appInstallation = await getAppInstallation(config, installationId);
    } catch (err) {
      throw githubErrorResponse(err);
    }
    if (!appInstallation) {
      throw new ApiError("installation_not_linked", 404);
    }
    // The linked row is the actual link; the installations row alone
    // (e.g. left behind by a half-finished link) must never mint.
    const linked = await findLinkedAccountByProviderAccount(
      client,
      user.id,
      String(appInstallation.account.id)
    );
    if (!linked || linked.auth_kind !== "app") {
      throw new ApiError("installation_not_linked", 404);
    }
    if (body.deviceIdentifier) {
      const device = await findDeviceByKey(
        client,
        user.id,
        body.deviceIdentifier
      );
      if (!device || device.revoked_at) {
        throw new ApiError("device_revoked", 403);
      }
    }
    const subscription = await findActiveSubscriptionForUser(client, user.id);
    const pkg =
      subscription?.package ?? (await findPackageByName(client, "Free"));
    if (!pkg) throw new ApiError("not_configured", 503);
    const rank = await linkedAccountRank(client, user.id, linked.id);
    if (rank === 0 || rank > pkg.max_linked_accounts) {
      throw new ApiError("limit_exceeded", 403, {
        max: pkg.max_linked_accounts,
      });
    }
    let minted;
    try {
      minted = await mintInstallationToken(config, installationId);
    } catch (err) {
      throw githubErrorResponse(err);
    }
    await recordAudit(client, {
      userId: user.id,
      action: "broker.mint",
      details: { installationId },
    });
    return NextResponse.json({
      token: minted.token,
      expiresAt: minted.expiresAt,
    });
  } catch (err) {
    return errorResponse(err);
  }
}
