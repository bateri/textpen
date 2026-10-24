import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The view is built into `dist/view`: many files, which a pencere terminal keeps by content
// (each language the editor reads is a file of its own, fetched when a file in it is first
// shown). `make build` packs them into the program.
export default defineConfig({
  // Relative, so that the view's files are found wherever its document is served from.
  base: "./",
  plugins: [react()],
  build: {
    outDir: "dist/view",
    emptyOutDir: true,
    chunkSizeWarningLimit: 1500,
  },
});
