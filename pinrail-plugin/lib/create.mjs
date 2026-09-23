// `pinrail-plugin create <name>`: a plugin folder from a template, asking
// nothing the name does not answer. The result runs under `dev`, passes its
// own tests and installs with --link before a line of it is changed.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { packageRoot } from "./paths.cjs";

const root = packageRoot(fileURLToPath(import.meta.url));
const templates = path.join(root, "templates");
const { version } = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));

export const TEMPLATES = ["plain", "vite", "react"];
const NAME = /^[a-z][a-z0-9_-]*$/;

/**
 * Where a scaffolded plugin gets the package from. Until it is on npm, the
 * tarball attached to the SDK's release of the same version; at the first
 * npm release this becomes a caret range.
 */
export const defaultSdkDep = () =>
  `https://github.com/forgeplane/pinrail/releases/download/sdk-v${version}/pinrail-plugin-${version}.tgz`;

/** "ticket_triage" → "Ticket triage" */
export const titleOf = (name) => name.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());

/** The files a template writes, relative to the plugin folder, in order. */
export function listTemplate(template) {
  const out = [];
  for (const layer of ["common", template]) {
    walk(path.join(templates, layer), "", out);
  }
  return out;
}

function walk(dir, rel, out) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const next = rel ? `${rel}/${entry.name}` : entry.name;
    if (entry.isDirectory()) walk(path.join(dir, entry.name), next, out);
    else out.push({ from: path.join(dir, entry.name), to: next });
  }
}

/** The path a template file lands at: placeholders filled, `_gitignore` made a dotfile. */
export function targetOf(rel, name) {
  return rel.replace(/__NAME__/g, name).replace(/(^|\/)_gitignore$/, "$1.gitignore");
}

/**
 * Writes the plugin. `opts`: `template` (one of TEMPLATES, "plain" by default), `dir` (default
 * `./<name>`), `sdk` (the package spec for package.json). Returns the folder
 * and the files written, relative to it.
 */
export function scaffold(name, opts = {}) {
  if (!NAME.test(name)) throw new Error(`a plugin's name is [a-z][a-z0-9_-]*: ${JSON.stringify(name)}`);
  const template = opts.template ?? "plain";
  if (!TEMPLATES.includes(template)) throw new Error(`no template ${JSON.stringify(template)}; one of ${TEMPLATES.join(", ")}`);
  const dir = path.resolve(opts.dir ?? name);
  if (fs.existsSync(dir) && fs.readdirSync(dir).length > 0) throw new Error(`${dir} exists and is not empty`);

  const fill = (text) =>
    text
      .replace(/__NAME__/g, name)
      .replace(/__TITLE__/g, titleOf(name))
      .replace(/__SDK_DEP__/g, opts.sdk ?? defaultSdkDep());

  const written = [];
  for (const { from, to } of listTemplate(template)) {
    const target = path.join(dir, targetOf(to, name));
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, fill(fs.readFileSync(from, "utf8")));
    written.push(path.relative(dir, target));
  }
  return { dir, written };
}

/** The command line: `create <name> [--template plain|vite|react] [--dir path] [--sdk spec]`. */
export function create(argv) {
  const args = [...argv];
  const flag = (key) => {
    const i = args.indexOf(key);
    if (i < 0) return undefined;
    const value = args[i + 1];
    args.splice(i, 2);
    return value;
  };
  const template = flag("--template");
  const dir = flag("--dir");
  const sdk = flag("--sdk");
  const name = args.find((a) => !a.startsWith("--"));
  if (!name) {
    console.error("usage: pinrail-plugin create <name> [--template plain|vite|react] [--dir path] [--sdk spec]");
    process.exit(2);
  }

  let result;
  try {
    result = scaffold(name, { template, dir, sdk });
  } catch (e) {
    console.error(String(e.message ?? e));
    process.exit(2);
  }

  const shown = path.relative(process.cwd(), result.dir) || ".";
  console.log(`${name} in ${shown}, from the ${template ?? "plain"} template:`);
  for (const file of result.written) console.log(`  ${file}`);
  console.log(`
next:
  cd ${shown} && npm install && npx playwright install chromium
  npx pinrail-plugin dev                 the view in a browser, on fixtures/basic.json${template && template !== "plain" ? "\n  npm run watch                         rebuilds view/ as you edit src/" : ""}
  npm test                              tests/ under the harness
  npx pinrail-plugin check               what the app would say of the folder
  pinrail plugins install . --link       the app serves the folder live`);
}
