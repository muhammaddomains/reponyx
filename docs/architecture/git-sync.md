# Git Synchronization Architecture

Status: Accepted (Phase 1) · Engine: Phase 3

## Modes

- **A — Mirror (default).** `git clone --mirror <url>` into
  `<root>/repositories/github/<account>/<owner>/<name>.git/`; sync with
  `git --git-dir=<path> fetch --prune`. Preserves objects, branches,
  tags, remote refs, full history. A bare repo is never treated as a
  working directory.
- **B — Working clone.** `git clone <url> <dest>`; sync fetches all
  remotes + tags and prunes, without overwriting uncommitted local
  changes. Divergence and dirty state are detected; fast-forward pull
  is opt-in. Never auto-reset or discard user modifications.
- **C — Both.** One mirror + one working clone, separate locations.

## Execution model (`crates/git-engine`)

- Git CLI is the execution engine (ADR-002). No shell: argument
  arrays only; controlled environment; per-operation timeout;
  cancellation token; structured, redacted output.
- Credentials are never in clone URLs or argv. A short-lived
  credential helper / controlled environment injects them per
  operation; secrets are redacted from all output and logs.
- Per-repository lock: one operation at a time per local path.
- Error mapping: exit codes + stderr are classified into stable
  `RepoNyxError` codes (`RepositoryNotFound`, `AuthenticationRevoked`,
  `NetworkUnavailable`, `DiskSpaceInsufficient`, `RepositoryLocked`,
  `RepositoryCorrupted`, …) with retryability + suggested action.

## LFS

A mirror does **not** guarantee every LFS object is archived. LFS sync
is a separate step (Phase 9): availability check, `lfs fetch`,
object verification where practical, failure reporting, storage
accounting. LFS failures never silently pass as success.

## Submodules

Optional recursive submodule sync (Phase 9). Submodule URLs are
detected; inaccessible private submodules fail gracefully per-repo.
A parent's authentication is never assumed to cover submodules.

## Storage layout

```text
<root>/                      (default D:\RepoNyx\ on Windows)
  repositories/github/<account>/<owner>/<name>.git/
  working-clones/<account>/<owner>/<name>/
  lfs/ logs/ exports/ backups/
  data/reponyx.sqlite  data/database-backups/
```

Path components (account/owner/name) are validated before use
(`git-engine::paths`): no separators, no `..`, no Windows-reserved
names, no illegal characters. Name collisions never silently overwrite
an unrelated local repository.

## Archival guarantees

A mirror is a Git history archive — not a complete backup of external
assets. LFS objects, submodules, GitHub releases, issues, wikis, and
Actions artifacts need their own handling (documented in Phase 9).
Local repos are never deleted when a remote is deleted, an account
disconnects, an installation is revoked, a PAT expires, or a repo is
deselected: the repo is marked `disconnected` / `remote_deleted` and
deletion requires explicit user confirmation.
