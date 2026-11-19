# ADR-001: Monorepo Architecture

- Status: Accepted (Phase 1)
- Date: 2026-11-18

## Context

RepoNyx spans a Tauri desktop app, a background agent, an auth broker,
shared Rust libraries, and shared TypeScript packages. A single repo
keeps the domain model, migrations, and API contracts in sync, but a
monolith package would tangle UI, Git, auth, and scheduling concerns.

## Decision

One monorepo with two workspaces:

- **pnpm workspaces** for `apps/desktop` and `packages/*`.
- **Cargo workspace** (resolver 2, edition 2024) for
  `apps/desktop/src-tauri` and `crates/*`.
- Workspace-level dependency management (`[workspace.dependencies]`)
  so versions are declared once and pinned in lockfiles.
- Only create a crate/package when it has a real responsibility; no
  empty scaffolding (planned crates are documented, not created).

## Consequences

- Shared types (`crates/core` / `packages/shared`) stay in lockstep.
- Builds are reproducible via `Cargo.lock` + `pnpm-lock.yaml`.
- Cross-package changes are atomic in one commit.
- Cost: workspace tooling (pnpm + cargo) must both be present for a
  full build; CI must run both.
