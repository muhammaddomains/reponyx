import { handleOAuthCallback } from "@/lib/oauth";
import { getDb } from "@/lib/db";

/** GitHub login callback — the exact URL registered in the GitHub OAuth app. */
export async function GET(req: Request) {
  return handleOAuthCallback(req, "github", getDb);
}
