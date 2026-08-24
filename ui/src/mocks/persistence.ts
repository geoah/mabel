// Where the mock node keeps what a visitor did, so a reload does not undo it.
// This module is part of the mock layer: only the mock store and the harness
// import it, so no bundle a user runs contains the key or the reset.

/** The one localStorage key the mock store writes. */
export const MOCK_STATE_KEY = "mabel.mock.state";

/**
 * Forgets what the visitor did. The next boot reseeds from the fixtures,
 * because the store reseeds itself when it finds no saved state.
 */
export function clearMockState(): void {
  try {
    globalThis.localStorage?.removeItem(MOCK_STATE_KEY);
  } catch {
    // Nothing was remembered, and the session still works.
  }
}

/** Throws away everything the visitor did and starts again from the fixtures. */
export function resetMockState(): void {
  clearMockState();
  globalThis.location?.reload();
}
