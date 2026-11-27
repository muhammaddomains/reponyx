-- 0003_add_user_name — full name is required at registration (W2 polish).
-- Never edit an applied migration; schema changes go in a new file.

ALTER TABLE users ADD COLUMN IF NOT EXISTS name text NULL;
