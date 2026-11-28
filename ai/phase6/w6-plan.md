# W6 plan — billing (Stripe first)

Status: IMPLEMENTED — Stripe end-to-end (W6a), local commit pending,
push = sync 86 (needs approval). Verified 2026-11-28: fmt clean;
clippy `-D warnings` clean; cargo 479/479 (no Rust changes); billing
package 17/17; database 20/20; web 53/53; desktop frontend 140/140
(unchanged); typecheck clean; lint clean; vite + next build clean;
i18n 269 keys 0 failures; Semgrep 0 (85 rules, 219 files); prettier
clean except the pre-existing `DashboardLiveRefresh.tsx`.
PayPal / Polar / Freemius checkout is the remaining W6 work (W6b).

## User decision (2026-11-28 ~07:45 MST)

"phir w6" — build W6 after W4-full + W5. The plan (§9) said "no
billing code until the user approves the business side" — the 07:45
order is that approval. The business-side prerequisites (provider
accounts, tax setup) are the user's to do when configuring providers:
**the code ships inert — no provider can sell until its env credentials
exist and the admin enables it, and no real charge is possible until
then.** Prices are 0 in the seed until billing; the admin sets the
price + the provider price ID per package before anything can be sold.

## Decisions (from plan §9, already made 2026-11-26)

- **Providers:** Stripe, PayPal, Polar, Freemius, Gumroad. Admin
  toggles which are active; a provider can be enabled only when its
  env values exist; keys are never shown in the admin UI — only
  whether they are set.
- **Env contract:** Stripe `STRIPE_SECRET_KEY` / `STRIPE_PUBLISHABLE_KEY`
  / `STRIPE_WEBHOOK_SECRET`; PayPal `PAYPAL_MODE=sandbox|live` +
  `PAYPAL_CLIENT_ID` / `PAYPAL_CLIENT_SECRET` / `PAYPAL_WEBHOOK_ID`;
  Polar `POLAR_ACCESS_TOKEN` / `POLAR_WEBHOOK_SECRET` /
  `POLAR_ENVIRONMENT=sandbox`; Freemius `FREEMIUS_PRODUCT_ID` /
  `FREEMIUS_API_KEY` / `FREEMIUS_SECRET_KEY` / `FREEMIUS_PUBLIC_KEY` /
  `FREEMIUS_PLAN_MAP` / `FREEMIUS_SANDBOX`; Gumroad `GUMROAD_APP_ID` /
  `GUMROAD_APP_SECRET`.
- **Gumroad is registered only — NO checkout.** It never appears at
  checkout regardless of country.
- **Country scope per selling provider:** all countries, or a list
  (admin-editable). Starting mapping: Stripe → worldwide. At checkout
  the buyer is only offered the providers active for their country
  (country from the `X-Vercel-IP-Country` header on Vercel, else the
  `CF-IPCountry` header, else the buyer's own selection).
- **Nothing is hardcoded.** Providers, toggles, country lists and the
  package → provider-price mapping are admin-managed data.
- **Rollout (§12): W6 = Stripe.** PayPal / Polar / Freemius checkout
  follows the same provider interface (W6b).

## Implementation (W6a — Stripe)

### 1. Migration 0006 (`packages/database/migrations/0006_billing.sql`)

- `subscriptions` += `provider` (text, null = pre-billing),
  `provider_subscription_id`, `provider_customer_id` (nullable text).
- `billing_providers` (provider text PK, is_active bool, countries
  jsonb null = worldwide, timestamps) — seeded with all five
  providers, all inactive.
- `package_provider_prices` (package_id FK, provider text,
  provider_price_id text, UNIQUE(package_id, provider)) — the
  provider's price/product id for a package, set by the admin.
- `billing_events` (provider text, event_id text, UNIQUE(provider,
  event_id), received_at) — webhook idempotency: a retried webhook
  is acknowledged without re-applying.

### 2. New package `packages/billing`

- `BillingProvider` interface: `key`, `displayName`,
  `supportsCheckout`, `isConfigured()` (env present),
  `createCheckoutSession({ user, packageName, priceId, successUrl,
  cancelUrl, customerId? })` → `{ url, providerSubscriptionId? }`,
  `verifyWebhook(rawBody, signature, webhookSecret)` → the
  provider's event, `normalizeEvent(event)` → `{ type:
  checkout_completed | subscription_updated | subscription_deleted,
  providerCustomerId, providerSubscriptionId, priceId, status,
  currentPeriodEnd } | null` (null = an event kind we ignore).
- Stripe implementation with the `stripe` SDK (latest stable,
  server-only): Checkout Sessions in `subscription` mode, webhook
  verification via `stripe.webhooks.constructEvent` (offline HMAC —
  testable without network).
- Registry: all five providers with their env-config checks;
  `availableProvidersForCountry(country, rows)` — active (admin) +
  configured (env) + `supportsCheckout` + country in scope (null =
  worldwide) + never Gumroad.
- Country: `buyerCountry(req)` — `X-Vercel-IP-Country`, else
  `CF-IPCountry`, else null (buyer picks).

### 3. Web routes + pages

- `GET /pricing` (public): active packages with limits + price
  (price_cents, formatted per locale) and a Subscribe button.
- `POST /api/billing/checkout` (session): {packageId, provider,
  country?} → package exists + active + not Free + has a provider
  price mapping + provider available for the country → create the
  Stripe Checkout Session → `{ url }`. Errors: `package_not_found`,
  `package_not_for_sale`, `provider_not_available`,
  `billing_not_configured`.
- `POST /api/billing/webhook/stripe`: raw body (`await req.text()`),
  signature from `stripe-signature`, `constructEvent` with
  `STRIPE_WEBHOOK_SECRET`; unknown event id → insert into
  `billing_events` (idempotency) → apply: `checkout.session.completed`
  → set the user's subscription to the mapped package, status active,
  expires_at = current period end, provider ids recorded;
  `customer.subscription.updated` → status/expires update;
  `customer.subscription.deleted` → status cancelled (keeps the
  package until expiry, then the limits fallback reverts to Free —
  the existing expiry rule). Audit rows (`billing.checkout`,
  `billing.webhook`).
- `GET /billing/success` + `/billing/cancel`: plain confirmation
  pages linking back to the dashboard.
- Admin: `/admin/billing` — per-provider card: configured? (env),
  active toggle, country scope editor, and the per-package provider
  price IDs. `GET/PATCH /api/admin/billing/providers` +
  `GET/PUT /api/admin/billing/package-prices`. Admin-only
  (requireAdmin).

### 4. i18n

New web domain `billing` (en + ur): pricing, checkout, success,
cancel, admin billing, provider names. Error keys in errors.json.

### 5. No desktop changes

The desktop already pulls package + limits from the web every 60s
(W4-full) — a subscription change on the web reaches the app within
a minute.

## Verification

- `cargo fmt --check`, clippy `-D warnings`, cargo test (excl.
  desktop) — no Rust changes, so the count stays 479.
- Database package tests (new: billing queries + idempotency),
  billing package tests (provider availability, country scopes,
  webhook normalization with a locally-signed Stripe payload),
  web tests (checkout route: unconfigured → 503, Free → 400,
  no price mapping → 400, happy path with a stubbed Stripe client;
  webhook: bad signature → 400, replayed event id → 200 without
  re-applying).
- `pnpm -r typecheck`, `pnpm -r lint`, vite build, next build,
  i18n validate, Semgrep (exact CI command), prettier (web +
  packages scope).
- Docs: CHANGELOG.md + `ai/memory/PROJECT_STATUS.md`; commit; push
  offer (sync 86).

## Remaining W6 work (W6b)

PayPal, Polar and Freemius checkout + webhooks through the same
`BillingProvider` interface (each needs its SDK, webhook
verification and subscription-lifecycle mapping), plus Gumroad
registration details (registered-only, no checkout — by design).
Needs the user's provider accounts + keys to test live; sandbox
keys let the webhooks be tested end to end before going live.
