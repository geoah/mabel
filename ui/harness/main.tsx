import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";

import { App } from "@/App";
import { Button } from "@/components/ui/button";
import { resetMockState } from "@/mocks/persistence";

import "./harness.css";

/**
 * The developer harness: the same wallet UI, served against the frozen fixtures
 * through the mock service worker, so no node has to be running. It exists for
 * two callers, the screenshot script and a developer looking at a screen.
 *
 * Nothing a user installs reaches this file. `ui/index.html` loads
 * `src/main.tsx`, which never imports the mocks, and only
 * `vite.harness.config.ts` has this directory as its root. For data in a real
 * node, run `mabel dev seed` against a real home instead.
 */
async function startMocks() {
  try {
    const { worker } = await import("@/mocks/browser");
    await worker.start({ onUnhandledRequest: "bypass" });
  } catch (error) {
    console.error("mock service worker failed to start", error);
  }
}

/**
 * The mock store remembers what a visitor did, so the harness needs one way to
 * put the fixtures back. The real wallet has no such control: it shows what its
 * node holds, and nothing here can put that back.
 */
function MockControls() {
  return (
    // The attribute is how a capture hides it: it belongs to no screen, so it
    // appears in no screenshot.
    <footer
      data-harness-controls
      className="mx-auto w-full max-w-2xl px-3 pb-20 sm:px-4 md:pb-4"
    >
      <div className="border-t pt-4">
        <Button
          type="button"
          variant="outline"
          size="sm"
          data-testid="mock-reset"
          onClick={resetMockState}
        >
          Reset the mock data
        </Button>
      </div>
    </footer>
  );
}

function render() {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <BrowserRouter>
        <App />
      </BrowserRouter>
      <MockControls />
    </StrictMode>,
  );
}

startMocks().finally(render);
