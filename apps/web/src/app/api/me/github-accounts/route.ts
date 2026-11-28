import { NextResponse } from "next/server";
import { z } from "zod";
import {
  countLinkedAccountsForUser,
  findActiveSubscriptionForUser,
  findInstallationByGithubId,
  findLinkedAccountByProviderAccount,
  findPackageByName,
  linkInstallation,
  linkPatAccount,
} from "@reponyx/database";
import { ApiError, errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";
import {
  getAppInstallation,
  getGitHubAppConfig,
  githubErrorResponse,
} from "@/lib/github-app";

const LinkBody = z.discriminatedUnion("authKind", [
  z.object({
    authKind: z.literal("app"),
    installationId: z.union([
      z.number().int().positive(),
      z.string().regex(/^\d+$/),
    ]),
  }),
  z.object({
    authKind: z.literal("pat"),
    login: z.string().min(1).max(128),
  }),
]);

/**
 * Link a GitHub account to the signed-in user (W4). For `app`, the
 * web verifies the installation against the shared App with its own
 * key and records both the installation → user link and the linked
 * account (keyed on the GitHub account id, stable across
 * reinstalls). For `pat`, only the login is recorded — the PAT
 * itself is never sent. Both kinds count toward the package limit;
 * linking an already-linked account is idempotent and returns it.
 */
export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, LinkBody);
    const client = getDb();
    const { user } = await requireSession(client, req);
    const subscription = await findActiveSubscriptionForUser(client, user.id);
    const pkg =
      subscription?.package ?? (await findPackageByName(client, "Free"));
    if (!pkg) throw new ApiError("not_configured", 503);

    if (body.authKind === "app") {
      const installationId =
        typeof body.installationId === "number"
          ? body.installationId
          : Number(body.installationId);
      const existing = await findInstallationByGithubId(client, installationId);
      if (existing && existing.user_id !== user.id) {
        throw new ApiError("already_linked", 409);
      }
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
      const already = await findLinkedAccountByProviderAccount(
        client,
        user.id,
        String(appInstallation.account.id)
      );
      if (already?.auth_kind === "app") {
        return NextResponse.json({ account: already });
      }
      const count = await countLinkedAccountsForUser(client, user.id);
      if (count >= pkg.max_linked_accounts) {
        throw new ApiError("limit_exceeded", 403, {
          max: pkg.max_linked_accounts,
        });
      }
      const { account } = await linkInstallation(client, {
        userId: user.id,
        githubInstallationId: installationId,
        accountId: String(appInstallation.account.id),
        login: appInstallation.account.login,
      });
      return NextResponse.json({ account });
    }

    const already = await findLinkedAccountByProviderAccount(
      client,
      user.id,
      body.login
    );
    if (already?.auth_kind === "pat") {
      return NextResponse.json({ account: already });
    }
    const count = await countLinkedAccountsForUser(client, user.id);
    if (count >= pkg.max_linked_accounts) {
      throw new ApiError("limit_exceeded", 403, {
        max: pkg.max_linked_accounts,
      });
    }
    const account = await linkPatAccount(client, {
      userId: user.id,
      login: body.login,
    });
    return NextResponse.json({ account });
  } catch (err) {
    return errorResponse(err);
  }
}
