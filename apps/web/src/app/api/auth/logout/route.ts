import { NextResponse } from "next/server";
import { hashToken } from "@reponyx/auth";
import {
  findDeviceById,
  recordAudit,
  recordLogout,
  revokeSession,
} from "@reponyx/database";
import { errorResponse } from "@/lib/api-errors";
import { clearSessionCookie, getSessionFromRequest } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp, hashIp } from "@/lib/rate-limits";

export async function POST(req: Request) {
  try {
    const client = getDb();
    const session = await getSessionFromRequest(client, req);
    if (session) {
      await revokeSession(client, hashToken(session.token));
      const device = session.session.device_id
        ? await findDeviceById(client, session.session.device_id)
        : null;
      const ip = clientIp(req);
      const ipHash = ip ? hashIp(ip) : null;
      await recordLogout(client, {
        userId: session.user.id,
        deviceKey: device?.device_key ?? null,
        ipHash,
      });
      await recordAudit(client, {
        userId: session.user.id,
        action: "auth.logout",
        ipHash,
      });
    }
    const res = NextResponse.json({ ok: true });
    res.headers.set("Set-Cookie", clearSessionCookie());
    return res;
  } catch (err) {
    return errorResponse(err);
  }
}
