"use client";

import { useEffect, useState } from "react";
import { errorMessage, postJson } from "./api";
import type { AuthFormProps } from "./RegisterForm";

export function VerifyClient({
  token,
  strings,
  errors,
  locale,
}: AuthFormProps & { token: string }) {
  const [state, setState] = useState<"pending" | "ok" | "failed">("pending");
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    postJson("/api/auth/verify", { token }).then((result) => {
      if (cancelled) return;
      if (result.ok) {
        setState("ok");
      } else {
        setState("failed");
        setMessage(errorMessage(errors, locale, result.data));
      }
    });
    return () => {
      cancelled = true;
    };
  }, [token, errors, locale]);

  if (state === "pending") return <p>{strings.loading}</p>;
  if (state === "ok") return <p>{strings.verifySuccess}</p>;
  return <p role="alert">{message ?? strings.verifyFailed}</p>;
}
