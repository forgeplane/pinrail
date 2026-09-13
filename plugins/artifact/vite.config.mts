import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The build lands at the plugin's root: index.html next to manifest.json,
// scripts and styles under assets/. Paths are relative, since the bundle is
// served under /plugins/artifact/<version>/. The output dir is not emptied:
// the manifest, the schemas and the fixtures live there too.
export default defineConfig({
  root: "src",
  base: "./",
  plugins: [react()],
  build: {
    outDir: "..",
    emptyOutDir: false,
    assetsDir: "assets",
    target: "es2022",
    modulePreload: { polyfill: false },
  },
});
