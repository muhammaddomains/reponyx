import { NextResponse } from "next/server";
import { z } from "zod";
import { listDevicesForUser, upsertDevice } from "@reponyx/database";
import { errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";
import { clientIp } from "@/lib/rate-limits";

const DeviceBody = z.object({
  deviceKey: z.string().min(1).max(128),
  machineName: z.string().max(128).optional(),
  platform: z.string().max(64).optional(),
  appVersion: z.string().max(64).optional(),
});

export async function GET(req: Request) {
  try {
    const client = getDb();
    const { user } = await requireSession(client, req);
    const devices = await listDevicesForUser(client, user.id);
    return NextResponse.json({ devices });
  } catch (err) {
    return errorResponse(err);
  }
}

export async function POST(req: Request) {
  try {
    const body = await parseJsonBody(req, DeviceBody);
    const client = getDb();
    const { user } = await requireSession(client, req);
    const device = await upsertDevice(client, {
      userId: user.id,
      deviceKey: body.deviceKey,
      machineName: body.machineName ?? null,
      platform: body.platform ?? null,
      appVersion: body.appVersion ?? null,
      lastIp: clientIp(req),
    });
    return NextResponse.json({ device });
  } catch (err) {
    return errorResponse(err);
  }
}
