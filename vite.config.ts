import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

import { harness } from "./harness/plugin.ts";

// The view is built into `dist/view`: many files, which a pencere terminal keeps by content
// (each language the editor reads is a file of its own, fetched when a file in it is first
// shown). `vite --mode harness` serves it in a stand-in terminal in the browser, with the
// real program behind it.
export default defineConfig(({ mode }) => ({
  // Relative, so that the view's files are found wherever its document is served from.
  base: "./",
  plugins: [react(), ...(mode === "harness" ? [harness()] : [])],
  build: {
    outDir: "dist/view",
    emptyOutDir: true,
    chunkSizeWarningLimit: 1500,
  },
  server: { port: 5198, strictPort: true },
}));
