import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// The dev server is also how you run this without Tauri: the daemon serves
// WebSocket and REST on localhost, so a browser is a perfectly good client.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // **Not** the Rust half. `npm run tauri dev` builds into
      // `src-tauri/target`, which is ~17k files of cargo fingerprints, and Vite
      // watches every one of them — on Linux that is 17k inotify handles
      // against a default limit of 65536 shared with every other watcher on the
      // machine. It fails as `ENOSPC` from `node:internal/fs/watchers`, which
      // reads as "disk full" and is nothing of the kind.
      //
      // Nothing here is an input to the frontend build, so there is nothing to
      // lose by not watching it. Tauri's own template ships this line; this
      // config was written without it and only broke once the Rust side had
      // been compiled enough times to matter.
      ignored: ["**/src-tauri/**"],
    },
  },
  // Tauri expects a fixed dist dir and no source maps in release.
  build: { outDir: "dist", target: "esnext", sourcemap: true },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
  },
});
