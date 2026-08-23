import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";

import { App } from "./App";
import "./index.css";

/**
 * Development and demo builds serve the wallet against the frozen fixtures
 * through the mock service worker, so no node has to be running. A regular
 * production build never registers it and talks to the node's own /api.
 *
 * Rendering never waits on the worker outcome: a failed registration logs
 * and the app still mounts, surfacing API errors instead of a blank page.
 */
async function startMocks() {
  if (!import.meta.env.DEV && import.meta.env.VITE_DEMO !== "1") {
    return;
  }
  try {
    const { worker } = await import("./mocks/browser");
    await worker.start({ onUnhandledRequest: "bypass" });
  } catch (error) {
    console.error("mock service worker failed to start", error);
  }
}

function render() {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </StrictMode>,
  );
}

startMocks().finally(render);
