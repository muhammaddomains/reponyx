import test from "node:test";
import assert from "node:assert/strict";
import { describeEmailConfig, resolveEmailProvider } from "../src/provider";

const resendEnv = { EMAIL_PROVIDER: "resend", RESEND_API_KEY: "re_test" };
const smtpEnv = {
  EMAIL_PROVIDER: "smtp",
  SMTP_HOST: "smtp.example.com",
  SMTP_USER: "user",
  SMTP_PASS: "pass",
};

test("explicit selector wins when both credential sets are present", () => {
  const both = {
    EMAIL_PROVIDER: "resend",
    RESEND_API_KEY: "re_test",
    SMTP_HOST: "smtp.example.com",
    SMTP_USER: "user",
    SMTP_PASS: "pass",
  };
  assert.equal(resolveEmailProvider(both), "resend");
  assert.equal(
    resolveEmailProvider({ ...both, EMAIL_PROVIDER: "smtp" }),
    "smtp"
  );
});

test("selector naming a provider without credentials resolves to none", () => {
  assert.equal(resolveEmailProvider({ EMAIL_PROVIDER: "resend" }), "none");
  assert.equal(resolveEmailProvider({ EMAIL_PROVIDER: "smtp" }), "none");
});

test("no selector falls back Resend, then SMTP, then none", () => {
  assert.equal(resolveEmailProvider({ RESEND_API_KEY: "re_test" }), "resend");
  assert.equal(
    resolveEmailProvider({ SMTP_HOST: "h", SMTP_USER: "u", SMTP_PASS: "p" }),
    "smtp"
  );
  assert.equal(resolveEmailProvider({}), "none");
});

test("describeEmailConfig never exposes keys", () => {
  const described = describeEmailConfig({
    ...resendEnv,
    ...smtpEnv,
    EMAIL_PROVIDER: "smtp",
  });
  assert.equal(described.provider, "smtp");
  assert.equal(described.resendConfigured, true);
  assert.equal(described.smtpConfigured, true);
  assert.equal(JSON.stringify(described).includes("re_test"), false);
});
