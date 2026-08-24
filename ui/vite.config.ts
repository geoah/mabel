import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const src = fileURLToPath(new URL("./src", import.meta.url));
const contracts = fileURLToPath(new URL("../contracts", import.meta.url));

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": src,
      "@contracts": contracts,
    },
  },
  server: {
    // `npm run dev` serves the wallet and nothing else: its /api requests reach
    // whatever this origin proxies to, so it needs a node behind it. For a
    // screen with no node, `npm run harness` uses vite.harness.config.ts.
    port: 5173,
    // Reachable over tailscale serve, which proxies with a .ts.net Host header.
    allowedHosts: [".ts.net"],
    // The UI calls /api on its own origin, and in dev its origin is this server,
    // so /api goes to the node a developer is running. MABEL_API moves it.
    proxy: { "/api": process.env.MABEL_API ?? "http://127.0.0.1:9080" },
    // contracts/ lives outside the Vite root, and the harness reads the frozen
    // documents from it.
    fs: { allow: [src, contracts, fileURLToPath(new URL(".", import.meta.url))] },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
  preview: {
    port: 4173,
    // Over tailscale serve, as above.
    allowedHosts: [".ts.net"],
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    css: false,
    restoreMocks: true,
  },
});
