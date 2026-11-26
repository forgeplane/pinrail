// Bundles the bench page into dist/: the page, and a chunk for each of the
// window's renderers, which the page imports only when it measures that
// renderer. A stylesheet a renderer imports becomes a <style> element, as
// the app's policy allows inline styles. libghostty-vt's WebAssembly is
// served by the fake core from where the app serves it.
import { build } from "esbuild";
import { readFileSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
export const dist = join(here, "dist");

/** Imports a stylesheet as a script that adds it to the page. */
const inlineStyles = {
  name: "inline-styles",
  setup(b) {
    b.onLoad({ filter: /\.css$/ }, (args) => ({
      contents: `const s = document.createElement("style"); s.textContent = ${JSON.stringify(readFileSync(args.path, "utf8"))}; document.head.append(s);`,
      loader: "js",
    }));
  },
};

/** Resolves a Vite `?url` import, such as the icon font, to the file,
 *  which esbuild copies into dist/ and imports as its URL. */
const urlImports = {
  name: "url-imports",
  setup(b) {
    b.onResolve({ filter: /\?url$/ }, (args) => ({
      path: join(args.resolveDir, args.path.slice(0, -"?url".length)),
    }));
  },
};

export async function buildBench({ quiet = false } = {}) {
  rmSync(dist, { recursive: true, force: true });
  await build({
    entryPoints: [join(here, "page.ts")],
    bundle: true,
    splitting: true,
    format: "esm",
    target: "es2022",
    minify: true,
    sourcemap: true,
    outdir: dist,
    logLevel: quiet ? "warning" : "info",
    plugins: [inlineStyles, urlImports],
    loader: { ".woff2": "file" },
    publicPath: "/dist",
    // as a production build of the app has it
    define: { "import.meta.env": "{}" },
  });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await buildBench();
