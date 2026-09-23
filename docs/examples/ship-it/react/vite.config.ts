import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The build lands in view/: index.html, the manifest's entry, with scripts
// and styles under view/assets/. Paths are relative, since the app serves the
// bundle under a path of its own. Only the build lives in view/, so it is
// emptied first.
export default defineConfig({
  root: "src",
  base: "./",
  plugins: [react()],
  build: {
    outDir: "../view",
    emptyOutDir: true,
    assetsDir: "assets",
    target: "es2022",
    modulePreload: { polyfill: false },
  },
});
