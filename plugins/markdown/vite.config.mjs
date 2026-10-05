import { defineConfig } from "vite";

// The build lands in view/: index.html, with its scripts under view/assets/
// and the icons from src/public/. Paths are relative, since the bundle is
// served under /plugins/markdown/<version>/. Mermaid is a chunk of its own,
// fetched only for a document with a diagram. Only the build lives in view/,
// so it is emptied first.
export default defineConfig({
  root: "src",
  base: "./",
  build: {
    outDir: "../view",
    emptyOutDir: true,
    assetsDir: "assets",
    target: "es2022",
    modulePreload: { polyfill: false },
    // mermaid's diagrams are large chunks by nature
    chunkSizeWarningLimit: 4096,
  },
});
