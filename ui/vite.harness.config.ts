import { fileURLToPath, URL } from "node:url";
import { mergeConfig } from "vite";

import base from "./vite.config.ts";

// The developer harness, and the only entry that reaches the mock service
// worker: root is ui/harness/, so the page loaded is harness/index.html and the
// worker script served at /mockServiceWorker.js is harness/public/. `npm run
// build` uses vite.config.ts, whose root holds neither, so no bundle a user runs
// can contain a fixture.
//
//   npm run harness      # port 4199, served from source
//   npm run screenshots  # builds ui/dist-harness/ and previews it
export default mergeConfig(base, {
  root: fileURLToPath(new URL("./harness", import.meta.url)),
  server: { port: 4199 },
  preview: { port: 4199 },
  // Beside ui/dist rather than inside it: `npm run build` empties ui/dist, and
  // a stale mock bundle must never be what a binary compiles in.
  build: { outDir: "../dist-harness", emptyOutDir: true },
});
