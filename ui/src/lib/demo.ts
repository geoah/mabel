// The demo build serves the wallet against the frozen fixtures through a mock
// service worker. This module names the one localStorage key that mock keeps its
// mutable state under, and the two things a screen may do with it, so a screen
// can offer the reset without importing the mock and pulling the fixtures into a
// production bundle.

import { writePreference } from "./preferences";

/** Where the mock node keeps what a visitor did, so a reload does not undo it. */
export const DEMO_STATE_KEY = "mabel.mock.state";

/** True in `npm run dev` and in a VITE_DEMO build, which are the mocked ones. */
export function isDemoMode(): boolean {
  return import.meta.env.DEV || import.meta.env.VITE_DEMO === "1";
}

/** Forgets what the visitor did. The next boot reseeds from the fixtures. */
export function clearDemoData(): void {
  writePreference(DEMO_STATE_KEY, null);
}

/**
 * Throws away everything the visitor did and starts again from the fixtures.
 * The reload is the reseed: the mock reseeds itself on boot when it finds no
 * saved state.
 */
export function resetDemoData(): void {
  clearDemoData();
  globalThis.location?.reload();
}
