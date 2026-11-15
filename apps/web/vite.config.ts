import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

const api = process.env.QUORUM_API_ORIGIN ?? "http://127.0.0.1:8080";

export default defineConfig({
  plugins: [react()],
  server: {
    host: "127.0.0.1",
    port: 5173,
    proxy: { "/api": api, "/health": api },
  },
  build: {
    target: "es2022",
    sourcemap: false,
    // Keep every asset as a separate file so the strict CSP (no inline) holds.
    assetsInlineLimit: 0,
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
