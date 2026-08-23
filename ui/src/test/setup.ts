import "@testing-library/jest-dom/vitest";

import { afterAll, afterEach, beforeAll } from "vitest";

import { server } from "@/mocks/server";
import { resetStore } from "@/mocks/store";

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));

afterEach(() => {
  server.resetHandlers();
  server.events.removeAllListeners();
  resetStore();
});

afterAll(() => server.close());
