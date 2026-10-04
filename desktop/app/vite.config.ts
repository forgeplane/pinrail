import { defineConfig, searchForWorkspaceRoot } from "vite";
import react from "@vitejs/plugin-react";
import license from "rollup-plugin-license";
import path from "node:path";

/** The licences a bundled npm package may carry: deny.toml's list, for the UI. */
const ALLOWED = [
  "0BSD",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "BlueOak-1.0.0",
  "CC0-1.0",
  "ISC",
  "MIT",
  "MIT-0",
  "MPL-2.0",
  "Unicode-3.0",
  "Unlicense",
  "Zlib",
];

// The app's side of the plugin protocol, from the SDK package beside the app.
const host = path.resolve(__dirname, "..", "..", "pinrail-plugin", "host");

// The UI lives in ui/; Tauri loads the built files from dist/. A production
// build also records every npm package that ends up in the bundle, with its
// licence text, in notices/npm.json for scripts/build-notices.mjs, and fails
// on a package under a licence outside ALLOWED.
export default defineConfig({
  root: "ui",
  resolve: { alias: { "@forgeplane/pinrail-plugin/host": path.join(host, "host.js") } },
  plugins: [
    react(),
    {
      ...license({
        thirdParty: {
          allow: { test: `(${ALLOWED.join(" OR ")})`, failOnUnlicensed: true, failOnViolation: true },
          output: {
            file: path.resolve(__dirname, "notices", "npm.json"),
            template: (dependencies) =>
              JSON.stringify(
                dependencies.map((d) => ({
                  name: d.name,
                  version: d.version,
                  license: d.license,
                  text: d.licenseText,
                  notice: d.noticeText,
                })),
                null,
                2,
              ),
          },
        },
      }),
      apply: "build",
    },
  ],
  clearScreen: false,
  server: { port: 5173, strictPort: true, fs: { allow: [searchForWorkspaceRoot(process.cwd()), host] } },
  build: { outDir: "../dist", emptyOutDir: true },
});
