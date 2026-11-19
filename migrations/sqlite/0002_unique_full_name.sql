-- RepoNyx V2 — enforce repository full_name uniqueness per account.
--
-- Why: the repository layer does collision detection in code, but
-- SQLite transactions are deferred by default, so two concurrent
-- upserts / renames could both pass the check and then both write,
-- leaving two repositories with the same full_name in one account.
-- A UNIQUE index makes the invariant hold at the storage layer; the
-- repository maps the constraint error to InvalidInput.
-- GitHub full names are unique per account, so no legitimate data can
-- violate this. V1 is untouched.
CREATE UNIQUE INDEX IF NOT EXISTS idx_repositories_account_full_name
    ON repositories (account_id, full_name);
