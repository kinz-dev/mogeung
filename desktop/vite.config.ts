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
  // Tauri expects a fixed dist dir, and gets no source maps in release.
  //
  // **`sourcemap: false`, since 2026-10-12.** This line said `true` from the
  // first React commit while the comment above it said the opposite, and the
  // cost was measured before it was turned off: the maps were 59 MB of a
  // 79 MB `dist`, which `tauri-codegen` compresses and embeds into the shell
  // binary at compile time — 11 MB of the 54 MB executable, about 5 s of
  // the Vite build and about 22 s of the shell's own compile, every install.
  // The trade is that a stack trace in the installed window's inspector
  // (`devtools` is on in release, `R-J89`) names a minified line rather than
  // a source one. The dev server is unaffected: it serves its own maps.
  //
  // Two pages: the window, and the arcade's window of its own
  // (`src/lib/arcadeWindow.ts`). `popout.html` is not one of them — it is
  // static, in `public/`, because it carries no script.
  build: {
    outDir: "dist",
    target: "esnext",
    sourcemap: false,
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, "index.html"),
        arcade: path.resolve(__dirname, "arcade.html"),
      },
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    // **Not because anything hangs.** A handful of these mount a terminal or
    // an editor under jsdom and take ~4s of honest work each, which against
    // the 5s default is a coin flip the moment the machine is also running the
    // dev server — the suite went from green to three timeouts and back purely
    // by adding a 57th test file. A timeout is meant to catch a test that has
    // stopped, not to race the one that is working.
    testTimeout: 20_000,
  },
});
