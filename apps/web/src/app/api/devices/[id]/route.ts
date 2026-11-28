import { NextResponse } from "next/server";
import { findDeviceById, revokeDevice } from "@reponyx/database";
import { ApiError, errorResponse } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";

/**
 * Revoke a device (W4). Revocation is the trigger for
 * device-revocation mirroring: the mint endpoint refuses revoked
 * devices, so revoking here also stops the broker minting tokens
 * for that device. Idempotent: revoking an already-revoked or
 * missing device returns `{ok: true}`.
 */
export async function DELETE(
  req: Request,
  props: { params: Promise<{ id: string }> }
) {
  try {
    const { id } = await props.params;
    const client = getDb();
    const { user } = await requireSession(client, req);
    const device = await findDeviceById(client, id);
    if (!device || device.user_id !== user.id) {
      return NextResponse.json({ ok: true });
    }
    if (!device.revoked_at) {
      await revokeDevice(client, id);
    }
    return NextResponse.json({ ok: true });
  } catch (err) {
    return errorResponse(err);
  }
}
