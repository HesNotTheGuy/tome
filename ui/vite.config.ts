import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Tauri expects a fixed port; fail if it's not available.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: false,
    // Tell Vite to ignore watching `src-tauri`; otherwise Rust changes
    // would trigger frontend rebuilds.
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
