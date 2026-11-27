# @reponyx/auth — password hashing + tokens (web)

- **Passwords:** Argon2id via `hash-wasm` (pure WebAssembly, no native
  binding) — the algorithm the plan requires, in an implementation that
  runs on Vercel (Node) and on Cloudflare Workers alike. Parameters:
  64 MiB memory, 3 iterations, parallelism 1 (OWASP first-party
  defaults). A malformed stored hash never crashes login — it just
  fails verification.
- **Tokens:** sessions, verification codes and reset tokens are random
  32-byte base64url strings; only their SHA-256 hash is stored.

Tests run through `tsx --test` (Node's built-in runner): plain
`node --test` cannot resolve extensionless relative imports in TS
sources, and this repo's convention is extensionless (the `.js`
extension broke Turbopack's re-export resolution in `packages/i18n`).

Convention: a package that imports Node built-ins needs
`"types": ["node"]` in its tsconfig — with TypeScript 6 + pnpm's
symlinked `node_modules`, `@types/node` is not auto-included.
