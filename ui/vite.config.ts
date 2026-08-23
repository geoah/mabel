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
    port: 5173,
    // Demo access over tailscale serve (proxies with a .ts.net Host header).
    allowedHosts: [".ts.net"],
    // The fixtures live outside the Vite root and are imported read-only.
    fs: { allow: [src, contracts, fileURLToPath(new URL(".", import.meta.url))] },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
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
