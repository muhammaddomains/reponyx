"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { errorMessage, postJson } from "./api";
import type { AuthFormProps } from "./RegisterForm";
import { alertError, alertSuccess, link } from "@/components/ui";

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
  if (state === "ok") {
    return (
      <div className="space-y-4">
        <p className={alertSuccess}>{strings.verifySuccess}</p>
        <Link href="/auth/login" className={link}>
          {strings.loginButton}
        </Link>
      </div>
    );
  }
  return (
    <p role="alert" className={alertError}>
      {message ?? strings.verifyFailed}
    </p>
  );
}
