import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// The dev server is also how you run this without Tauri: the daemon serves
// WebSocket and REST on localhost, so a browser is a perfectly good client.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  server: { port: 1420, strictPort: true },
  // Tauri expects a fixed dist dir and no source maps in release.
  build: { outDir: "dist", target: "esnext", sourcemap: true },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
  },
});
