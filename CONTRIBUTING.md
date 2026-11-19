# Contributing to RepoNyx

## Workflow

- Work on a feature branch off `main` (e.g. `feature/reponyx-foundation`).
  Never commit directly to `main`; never merge to `main` without explicit
  instruction. Never force-push, never rewrite history.
- Keep changes small and phase-scoped; see `ai/phaseN/PLAN.md` for the
  current phase's acceptance criteria.
- Run before opening a PR: `cargo fmt --check`, `cargo clippy --workspace
  --all-targets`, `cargo test --workspace`, `pnpm -r typecheck`, and the
  desktop `vite build`.

## Rules (from the product spec, §23)

- No placeholder functionality that appears implemented. No buttons that do
  nothing, no fake sync results, no mock data in production paths. Mocks
  belong in tests and clearly labelled dev previews.
- Never hardcode secrets. Never store PATs as plaintext. Never put tokens
  in Git remote URLs, command-line arguments, or logs. Never execute
  user-controlled shell commands — Git is invoked via argument arrays only.
- Never trust repository names as filesystem paths; validate and sanitize
  every path component (`crates/git-engine` path module).
- Never silently delete local repositories. Remote deletion / revocation
  marks a repo disconnected; deletion needs explicit user confirmation.
- Report test results with actual counts. Never claim a test passed without
  running it. Never claim "100% complete" or "production-ready" without
  verifiable evidence.

## Code style

- Rust: `edition 2024`, `cargo fmt`, Clippy clean. Small focused modules;
  no god objects; explicit error handling (`Result<T, RepoNyxError>`).
- TypeScript: strict mode, no `any` without justification, shared types in
  `packages/shared` — do not duplicate domain types in the frontend.
- Docs: update `CHANGELOG.md` and `ai/memory/PROJECT_STATUS.md` at the end
  of every phase; update ADRs when an architecture decision changes.

## Security

See `SECURITY.md`. Report vulnerabilities privately; do not open a public
issue with exploit details.
