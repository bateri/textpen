import { fileURLToPath } from "node:url";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The view is the page in `src/view`, built into `dist/view`: many files, which a pencere terminal
// keeps by content (each language the editor reads is a file of its own, fetched when a file in it
// is first shown). `make build` packs them into the program with `pencere build --web dist/view`.
export default defineConfig({
  root: "src/view",
  // Relative, so that the view's files are found wherever its document is served from.
  base: "./",
  plugins: [react()],
  build: {
    // Beside `src/`, not under the root of the page: nothing generated lives in `src/`.
    outDir: fileURLToPath(new URL("./dist/view", import.meta.url)),
    emptyOutDir: true,
    chunkSizeWarningLimit: 1500,
  },
});
