import { NextResponse } from "next/server";
import { z } from "zod";
import { updateUserName } from "@reponyx/database";
import { errorResponse, parseJsonBody } from "@/lib/api-errors";
import { requireSession } from "@/lib/auth";
import { getDb } from "@/lib/db";

const PatchBody = z.object({
  name: z.string().trim().min(1).max(128),
});

/** Update the signed-in user's full name (completes the profile). */
export async function PATCH(req: Request) {
  try {
    const body = await parseJsonBody(req, PatchBody);
    const client = getDb();
    const { user } = await requireSession(client, req);
    const updated = await updateUserName(client, {
      userId: user.id,
      name: body.name,
    });
    return NextResponse.json({
      user: { id: updated.id, email: updated.email, name: updated.name },
    });
  } catch (err) {
    return errorResponse(err);
  }
}
