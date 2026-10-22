import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

import { harness } from "./harness/plugin.ts";

const { version } = JSON.parse(readFileSync(new URL("package.json", import.meta.url), "utf8"));

// The view is built twice, into two directories: `vite build` writes many files, which a
// pencere terminal keeps by content (each language the editor reads is a file of its own,
// fetched when a file in it is first shown), and `vite build --mode single` writes one
// document, which every other MCP Apps host reads. `vite --mode harness` serves it in a
// stand-in host in the browser, with the real program behind it.
export default defineConfig(({ mode }) => {
  const single = mode === "single";
  return {
    // Relative, so that the view's files are found wherever its document is served from.
    base: "./",
    // The view tells its host which version it is; a release changes it in package.json only.
    define: { __TEXTPEN_VERSION__: JSON.stringify(version) },
    plugins: [
      react(),
      ...(single ? [viteSingleFile()] : []),
      ...(mode === "harness" ? [harness()] : []),
    ],
    build: {
      outDir: single ? "dist/single" : "dist/view",
      emptyOutDir: true,
      chunkSizeWarningLimit: 1500,
    },
    server: { port: 5198, strictPort: true },
  };
});
