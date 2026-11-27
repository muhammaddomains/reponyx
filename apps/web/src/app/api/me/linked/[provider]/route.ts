import { NextResponse } from "next/server";
import {
  deleteLinkedProvider,
  listLinkedProvidersForUser,
} from "@reponyx/database";
import { ApiError, errorResponse } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";

export async function DELETE(
  req: Request,
  props: { params: Promise<{ provider: string }> }
) {
  try {
    const { provider } = await props.params;
    if (provider !== "google" && provider !== "github") {
      throw new ApiError("invalid_body", 400);
    }
    const client = getDb();
    const { user } = await requireSession(client, req);
    const linked = await listLinkedProvidersForUser(client, user.id);
    const target = linked.find((row) => row.provider === provider);
    if (!target) return NextResponse.json({ ok: true });
    // Never remove the last way in: an account with no password and a
    // single linked provider would be locked out.
    if (linked.length === 1 && !user.has_password) {
      throw new ApiError("last_login_method", 409);
    }
    await deleteLinkedProvider(client, user.id, provider);
    return NextResponse.json({ ok: true });
  } catch (err) {
    return errorResponse(err);
  }
}
