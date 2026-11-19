import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

// Tauri dev server port must match tauri.conf.json `devUrl`.
export default defineConfig({
  build: {
    target: "es2022",
  },
  clearScreen: false,
  plugins: [react(), tailwindcss()],
  server: {
    port: 1420,
    strictPort: true,
  },
});
