-- 0006_billing — billing providers, package → provider price mapping,
-- subscription provider ids, and webhook idempotency (W6).
-- Providers, toggles, country scopes and price mappings are all
-- admin-managed data; nothing is hardcoded. A provider can only sell
-- when its env credentials exist (checked in code) and the admin has
-- enabled it here.

-- Provider ids recorded on a subscription (null = pre-billing rows).
ALTER TABLE subscriptions
  ADD COLUMN IF NOT EXISTS provider text NULL,
  ADD COLUMN IF NOT EXISTS provider_subscription_id text NULL,
  ADD COLUMN IF NOT EXISTS provider_customer_id text NULL;

-- Billing providers: one row per provider, admin-managed.
CREATE TABLE IF NOT EXISTS billing_providers (
  provider text PRIMARY KEY,
  is_active boolean NOT NULL DEFAULT false,
  countries jsonb NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- Seed all five providers, all inactive. countries NULL = worldwide.
INSERT INTO billing_providers (provider, is_active, countries)
VALUES
  ('stripe', false, NULL),
  ('paypal', false, NULL),
  ('polar', false, NULL),
  ('freemius', false, NULL),
  ('gumroad', false, NULL)
ON CONFLICT (provider) DO NOTHING;

-- The provider's price/product id for a package, set by the admin.
-- A package with no row for a provider cannot be sold through it.
CREATE TABLE IF NOT EXISTS package_provider_prices (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  package_id uuid NOT NULL REFERENCES packages (id) ON DELETE CASCADE,
  provider text NOT NULL,
  provider_price_id text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (package_id, provider)
);
CREATE INDEX IF NOT EXISTS idx_package_provider_prices_package
  ON package_provider_prices (package_id);

-- Webhook idempotency: a retried webhook (same provider + event id)
-- is acknowledged without re-applying its effect.
CREATE TABLE IF NOT EXISTS billing_events (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  provider text NOT NULL,
  event_id text NOT NULL,
  received_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (provider, event_id)
);
