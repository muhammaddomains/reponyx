## Summary

<!-- What does this PR change, and why? Link related issues, e.g. "Fixes #123", or the phase / PLAN section it implements. -->

## Type of change

- [ ] Bug fix
- [ ] New feature / enhancement
- [ ] Desktop UI (`apps/desktop/src/`, `packages/`)
- [ ] Desktop backend / Rust crates (`apps/desktop/src-tauri/`, `crates/`)
- [ ] Background agent (`apps/background-agent/`)
- [ ] Auth Broker (`apps/auth-broker/`)
- [ ] Documentation
- [ ] CI / workflows / tooling
- [ ] Chore / refactor

## Testing done

<!-- Commands run and their results. All of these must pass before merge.
The desktop Rust crate cannot be compiled on Linux (missing
WebViewGTK / gdk-3.0) — its first real compile is the Windows build,
so note it explicitly when desktop Rust changes. -->

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --workspace --exclude reponyx-desktop --all-targets` (0 warnings)
- [ ] `cargo test --workspace --exclude reponyx-desktop` (paste the passed/failed counts)
- [ ] `pnpm -r typecheck`
- [ ] `pnpm --filter @reponyx/desktop build`
- [ ] `pnpm --filter @reponyx/desktop exec eslint <touched files>`
- [ ] Windows build checked (`pnpm tauri:dev` / `pnpm tauri:build`) — required when desktop Rust changed

<!-- Paste the relevant output, or describe what you verified manually
(e.g. ran the agent live against a local bare repo, tested the built
installer, tested a setting change on Windows). -->

## Screenshots

<!-- Required for any UI change: before/after. Delete this section if the PR has no visual change. -->

## Checklist

- [ ] No secrets, private keys, tokens, or real `.env` files committed.
- [ ] GitHub App private key stays in the Auth Broker only — never in the desktop binary, frontend, logs, or SQLite.
- [ ] PATs stay in the OS credential store only — never plaintext in SQLite, JSON, or logs.
- [ ] No mock data, placeholder buttons, or fake sync results in production paths.
- [ ] No auto-delete of local repositories on remote deletion, disconnect, or deselection.
- [ ] Latest stable versions only — no beta / alpha / release-candidate dependencies.
- [ ] Docs updated if behavior changed (`CHANGELOG.md` and `ai/memory/PROJECT_STATUS.md` at phase end; `ai/phaseN/PLAN.md` as appropriate).
- [ ] No production deploy triggered or performed by this PR (releases are tag-triggered only).
