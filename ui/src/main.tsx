import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";

import { App } from "./App";
import "./index.css";

/**
 * The wallet, talking to the node that served it: every request goes to /api on
 * this same origin. There is no fixture, no mock and no offline mode in this
 * entry, in dev or in a build. To develop against data, run `mabel dev seed`
 * against a home and serve it; to look at a screen with no node at all, run
 * `npm run harness`.
 */
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </StrictMode>,
);
