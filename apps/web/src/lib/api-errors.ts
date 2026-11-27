import { NextResponse } from "next/server";
import { z } from "zod";

/** Structured errors (plan §7): every rejection carries a code + data, never a user-facing sentence — the client localizes it from errors.json. */
export class ApiError extends Error {
  constructor(
    public code: string,
    public status: number,
    public data?: Record<string, unknown>,
  ) {
    super(code);
  }
}

export function errorResponse(err: unknown): NextResponse {
  if (err instanceof ApiError) {
    return NextResponse.json(
      { error: { code: err.code, data: err.data ?? {} } },
      { status: err.status },
    );
  }
  console.error(err);
  return NextResponse.json(
    { error: { code: "internal_error", data: {} } },
    { status: 500 },
  );
}

export async function parseJsonBody<T>(req: Request, schema: z.ZodType<T>): Promise<T> {
  let body: unknown;
  try {
    body = await req.json();
  } catch {
    throw new ApiError("invalid_body", 400);
  }
  const parsed = schema.safeParse(body);
  if (!parsed.success) {
    throw new ApiError("invalid_body", 400, {
      issues: parsed.error.issues.map((issue) => issue.path.join(".")),
    });
  }
  return parsed.data;
}
