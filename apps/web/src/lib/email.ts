import {
  createEmailSender,
  passwordResetEmail,
  verificationEmail,
  type EmailEnv,
  type EmailSender,
} from "@reponyx/email";

let sender: EmailSender | null = null;
let override: EmailSender | null = null;

/** Test-only: replace the email sender (used by the route-flow tests). */
export function setEmailSenderForTests(next: EmailSender | null): void {
  override = next;
}

export function getEmailSender(): EmailSender {
  if (override) return override;
  if (!sender) sender = createEmailSender(process.env as unknown as EmailEnv);
  return sender;
}

function appUrl(): string {
  return process.env.NEXT_PUBLIC_APP_URL ?? "http://localhost:3000";
}

export async function sendVerificationEmail(
  to: string,
  token: string
): Promise<void> {
  const url = `${appUrl()}/auth/verify?token=${encodeURIComponent(token)}`;
  await getEmailSender().send({ to, ...verificationEmail(url) });
}

export async function sendPasswordResetEmail(
  to: string,
  token: string
): Promise<void> {
  const url = `${appUrl()}/auth/reset/confirm?token=${encodeURIComponent(token)}`;
  await getEmailSender().send({ to, ...passwordResetEmail(url) });
}
