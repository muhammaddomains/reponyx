import test from "node:test";
import assert from "node:assert/strict";
import nodemailer from "nodemailer";
import { createEmailSender } from "../src/send";

test("Resend path posts to api.resend.com with Bearer auth", async () => {
  const seen: { url: string; init: RequestInit }[] = [];
  const fetchImpl = (async (url: string, init: RequestInit) => {
    seen.push({ url, init });
    return new Response(JSON.stringify({ id: "resend-123" }), { status: 200 });
  }) as typeof fetch;
  const sender = createEmailSender(
    { EMAIL_PROVIDER: "resend", RESEND_API_KEY: "re_test", RESEND_FROM: "RepoNyx <hi@reponyx.bond>" },
    { fetchImpl },
  );
  const sent = await sender.send({ to: "a@example.com", subject: "Hi", html: "<p>Hi</p>" });
  assert.equal(sent.provider, "resend");
  assert.equal(sent.messageId, "resend-123");
  assert.equal(seen.length, 1);
  assert.equal(seen[0].url, "https://api.resend.com/emails");
  const headers = seen[0].init.headers as Record<string, string>;
  assert.equal(headers.Authorization, "Bearer re_test");
  const body = JSON.parse(String(seen[0].init.body));
  assert.equal(body.to, "a@example.com");
  assert.equal(body.from, "RepoNyx <hi@reponyx.bond>");
});

test("Resend failure throws with the status", async () => {
  const fetchImpl = (async () => new Response("bad key", { status: 401 })) as typeof fetch;
  const sender = createEmailSender(
    { EMAIL_PROVIDER: "resend", RESEND_API_KEY: "re_bad" },
    { fetchImpl },
  );
  await assert.rejects(sender.send({ to: "a@example.com", subject: "Hi", html: "Hi" }), /401/);
});

test("SMTP path sends through the transport", async () => {
  const transport = nodemailer.createTransport({ streamTransport: true, buffer: true });
  const sender = createEmailSender(
    { EMAIL_PROVIDER: "smtp", SMTP_HOST: "h", SMTP_USER: "u", SMTP_PASS: "p" },
    { smtpTransport: transport },
  );
  const sent = await sender.send({ to: "a@example.com", subject: "Hi", html: "<p>Hi</p>" });
  assert.equal(sent.provider, "smtp");
  assert.match(sent.messageId, /@/);
});

test("no provider configured throws a clear error", () => {
  assert.throws(() => createEmailSender({}), /No email provider configured/);
});
