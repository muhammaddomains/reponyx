import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

/** Frontend tests (Phase 8): Vitest + Testing Library over jsdom.
 *  The workspace packages (`@reponyx/shared`, `@reponyx/ui`,
 *  `@reponyx/api-client`) export their TypeScript source directly,
 *  so Vite/Vitest transforms them like first-party code — no
 *  aliasing or build step is needed. Tauri commands are mocked per
 *  test with `vi.mock("@reponyx/api-client")`; nothing here talks
 *  to a real backend. Run with `pnpm --filter @reponyx/desktop
 *  test`; CI runs the same step in the frontend job. */
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: false,
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test/setup.ts"],
  },
});
