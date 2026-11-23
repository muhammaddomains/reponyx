import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

/** Vitest runs with `globals: false`, so Testing Library cannot
 *  register its own auto-cleanup — unmount every render after
 *  each test, or queries in later tests see earlier renders. */
afterEach(() => {
  cleanup();
});

/** jsdom has no `window.matchMedia`; the theme code reads it for
 *  the `system` setting, so stub it globally. Tests that care about
 *  the OS preference override it with `vi.stubGlobal` themselves. */
window.matchMedia = window.matchMedia ?? ((query: string) => ({
  matches: false,
  media: query,
  onchange: null,
  addListener: () => {},
  removeListener: () => {},
  addEventListener: () => {},
  removeEventListener: () => {},
  dispatchEvent: () => false,
}));

/** jsdom has no ResizeObserver; the virtualized Repositories list
 *  measures rows with one, so stub it. Real browsers measure for
 *  real — this only keeps the tests honest about rendering. */
class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver = globalThis.ResizeObserver ?? ResizeObserverStub;
