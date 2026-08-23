import "@testing-library/jest-dom/vitest";

import { afterAll, afterEach, beforeAll } from "vitest";

import { server } from "@/mocks/server";
import { resetStore } from "@/mocks/store";

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));

afterEach(() => {
  server.resetHandlers();
  server.events.removeAllListeners();
  resetStore();
  // Developer mode, the selected identity and the consents live in
  // localStorage, so one test's choice must not reach the next.
  globalThis.localStorage?.clear();
});

afterAll(() => server.close());
