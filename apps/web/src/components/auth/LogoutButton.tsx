"use client";

export function LogoutButton({ label }: { label: string }) {
  async function onClick() {
    await fetch("/api/auth/logout", { method: "POST" });
    window.location.href = "/auth/login";
  }
  return (
    <button type="button" onClick={onClick}>
      {label}
    </button>
  );
}
