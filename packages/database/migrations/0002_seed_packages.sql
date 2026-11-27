-- 0002_seed_packages — the starting package set from the plan (§6).
-- Packages are admin-managed; this only seeds the starting set.
-- Prices are 0 until billing (W6) is approved.

INSERT INTO packages (name, max_linked_accounts, max_devices, price_cents)
VALUES
  ('Free', 2, 2, 0),
  ('Bronze', 10, 10, 0),
  ('Silver', 20, 20, 0),
  ('Gold', 50, 50, 0),
  ('Diamond', 100, 100, 0)
ON CONFLICT (name) DO NOTHING;
