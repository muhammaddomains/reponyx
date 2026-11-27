import nodemailer, { type Transporter } from "nodemailer";
import { type EmailEnv, resolveEmailProvider, type EmailProvider } from "./provider";

export interface EmailMessage {
  to: string;
  subject: string;
  html: string;
  text?: string;
}

export interface SentEmail {
  messageId: string;
  provider: EmailProvider;
}

export interface EmailSender {
  provider: EmailProvider;
  send(message: EmailMessage): Promise<SentEmail>;
}

export interface EmailSenderDeps {
  fetchImpl?: typeof fetch;
  smtpTransport?: Transporter;
}

const RESEND_TIMEOUT_MS = 15_000;

/** One send path for all platform email (verification, reset, notifications). */
export function createEmailSender(env: EmailEnv, deps: EmailSenderDeps = {}): EmailSender {
  const provider = resolveEmailProvider(env);
  if (provider === "none") {
    throw new Error(
      "No email provider configured: set EMAIL_PROVIDER=resend with RESEND_API_KEY, or EMAIL_PROVIDER=smtp with SMTP_HOST/SMTP_USER/SMTP_PASS.",
    );
  }
  if (provider === "resend") {
    const fetchImpl = deps.fetchImpl ?? fetch;
    const apiKey = env.RESEND_API_KEY as string;
    const from = env.RESEND_FROM ?? "RepoNyx <noreply@reponyx.bond>";
    return {
      provider,
      async send(message) {
        const response = await fetchImpl("https://api.resend.com/emails", {
          method: "POST",
          headers: {
            Authorization: `Bearer ${apiKey}`,
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            from,
            to: message.to,
            subject: message.subject,
            html: message.html,
            text: message.text,
          }),
          signal: AbortSignal.timeout(RESEND_TIMEOUT_MS),
        });
        if (!response.ok) {
          const detail = await response.text().catch(() => "");
          throw new Error(`Resend send failed (${response.status}): ${detail}`.trim());
        }
        const body = (await response.json()) as { id?: string };
        return { messageId: body.id ?? "resend-unknown", provider };
      },
    };
  }
  const transport =
    deps.smtpTransport ??
    nodemailer.createTransport({
      host: env.SMTP_HOST,
      port: Number(env.SMTP_PORT ?? 587),
      secure: Number(env.SMTP_PORT ?? 587) === 465,
      auth: { user: env.SMTP_USER, pass: env.SMTP_PASS },
    });
  const from = env.SMTP_FROM ?? env.SMTP_USER ?? "RepoNyx <noreply@reponyx.bond>";
  return {
    provider,
    async send(message) {
      const info = await transport.sendMail({
        from,
        to: message.to,
        subject: message.subject,
        html: message.html,
        text: message.text,
      });
      return { messageId: info.messageId ?? "smtp-unknown", provider };
    },
  };
}
