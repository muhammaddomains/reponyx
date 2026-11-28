import { NextResponse } from "next/server";
import { unlinkLinkedAccount } from "@reponyx/database";
import { errorResponse } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";

/**
 * Unlink a GitHub account (W4). Idempotent: unlinking an account
 * that is not linked returns `{ok: true}`, like the sign-in-method
 * unlink. The desktop calls this when an account is removed or
 * deleted, so the web count follows the desktop.
 */
export async function DELETE(
  req: Request,
  props: { params: Promise<{ id: string }> }
) {
  try {
    const { id } = await props.params;
    const client = getDb();
    const { user } = await requireSession(client, req);
    await unlinkLinkedAccount(client, { userId: user.id, id });
    return NextResponse.json({ ok: true });
  } catch (err) {
    return errorResponse(err);
  }
}
