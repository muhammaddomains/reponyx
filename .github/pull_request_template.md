## Summary

<!-- What does this PR change, and why? Link related issues, e.g. "Fixes #123". -->
<!-- Note: GitHub Issues are disabled in this repo — link a PR, commit, or SPEC section instead where possible. -->

## Type of change

- [ ] Bug fix
- [ ] New feature / enhancement
- [ ] Frontend (`src/`)
- [ ] Backend (`src-tauri/`)
- [ ] Documentation
- [ ] CI / workflows / tooling
- [ ] Chore / refactor

## Testing done

<!-- Commands run and their results. All of these must pass: -->

- [ ] `pnpm install --frozen-lockfile`
- [ ] `pnpm run test:unit` (vitest)
- [ ] `pnpm run test:rust` (`cargo test`)
- [ ] `pnpm run test:e2e` (Playwright, Windows + Edge)
- [ ] `pnpm run build` (production build, exit 0)

<!-- Paste the relevant output, or describe what you verified manually
(e.g. tested the built installer, tested the session screen, tested a
setting change, tested a mascot send). -->

## Screenshots

<!-- Required for any UI change: before/after. Delete this section if the PR has no visual change. -->

## Checklist

- [ ] No hardcoded single-user branding — the change works for any user of this app.
- [ ] No change to other projects (this repo only).
- [ ] No production deploy triggered or performed by this PR (releases are tag-triggered only).
- [ ] Docs updated if behavior or config changed (`README.md`, `docs/SPEC.md`, `docs/CUSTOMIZE.md`, `release.md`, or `CHANGELOG.md` as appropriate).
- [ ] No avatar videos (`.mp4`) committed — only `avatars/setting.json` and `avatars/plugin/quotes.json` are tracked.
- [ ] No secrets, keys, or real `.env` files committed.
