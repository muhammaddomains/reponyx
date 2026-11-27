import { handleOAuthCallback } from "@/lib/oauth";
import { getDb } from "@/lib/db";

/** Google login callback — the exact URL registered in the Google OAuth app. */
export async function GET(req: Request) {
  return handleOAuthCallback(req, "google", getDb);
}
