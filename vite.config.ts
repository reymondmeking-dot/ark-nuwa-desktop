import { defineConfig } from "vite";

// Tauri expects a fixed port and to not clear the screen during dev.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Don't watch the Rust side — target/ churns during compilation and
      // crashes the dev watcher with EBUSY on Windows.
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "es2020",
    outDir: "dist",
  },
});
