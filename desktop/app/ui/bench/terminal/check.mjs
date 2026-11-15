// Runs the input and selection checks (runners/wkcheck.swift) on each
// renderer in the system WKWebView, and writes results/checks-<time>.md.
//
//   node check.mjs [--adapters ghostty,xterm]
//
// macOS only. The check window takes focus while it runs.

import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, statSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { startCore } from "./core/server.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const arg = (name, d) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : d;
};
const ADAPTERS = arg("adapters", "ghostty,xterm").split(",");

const src = join(here, "runners", "wkcheck.swift");
const bin = join(here, ".cache", "wkcheck");
if (!existsSync(bin) || statSync(bin).mtimeMs < statSync(src).mtimeMs) {
  mkdirSync(dirname(bin), { recursive: true });
  if (spawnSync("swiftc", ["-O", src, "-o", bin], { stdio: "inherit" }).status !== 0) throw new Error("swiftc failed");
}
if (!existsSync(join(here, "corpus", "synthetic", "tui.bin")))
  spawnSync(process.execPath, [join(here, "corpus", "tools", "gen.mjs")], { stdio: "inherit" });
await (await import("./build.mjs")).buildBench({ quiet: true });
const core = await startCore({ port: 0 });
const shots = join(here, "results", "checks");
mkdirSync(shots, { recursive: true });

const results = {};
for (const adapter of ADAPTERS) {
  const dir = join(shots, adapter);
  mkdirSync(dir, { recursive: true });
  const url = `${core.url}/page.html?adapter=${adapter}&case=live`;
  // Not spawnSync: the fake core runs in this process and must keep serving.
  const r = await new Promise((resolve) => {
    const child = spawn(bin, [url, "1400", "760", dir]);
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (d) => (stdout += d));
    child.stderr.on("data", (d) => (stderr += d));
    const timer = setTimeout(() => child.kill(), 150000);
    child.on("close", (status) => {
      clearTimeout(timer);
      resolve({ stdout, stderr, status });
    });
  });
  try {
    results[adapter] = JSON.parse(r.stdout.trim().split("\n").pop());
  } catch {
    // stderr ends with the step that was running
    results[adapter] = {
      error: (r.stderr || r.stdout || `exit ${r.status}`).trim().split("\n").slice(-2).reverse().join(" after "),
    };
  }
  console.error(`${adapter}: ${results[adapter].error ?? "done"}`);
}
await core.close?.();

const names = [
  ...new Set(Object.values(results).flatMap((r) => Object.keys(r).filter((k) => r[k]?.detail !== undefined))),
];
const mark = (c) =>
  c === undefined ? "–" : c.pass === true ? "pass" : c.pass === false ? `**FAIL** ${c.detail}` : c.detail;
const unchecked = (name, a) =>
  results[a][name]?.pass == null && /^select/.test(name) && results[a]["DOM buttons while dragging"] === 0;
let md = `# Input and selection checks, ${new Date().toISOString()}\n\n`;
md += "System WKWebView, real AppKit key, input method and mouse events (runners/wkcheck.swift). ";
md += `Keyboard layout: ${Object.values(results)[0]?.keyboardLayout ?? "?"}.\n\n`;
md += `| Check | ${ADAPTERS.join(" | ")} |\n| --- | ${ADAPTERS.map(() => "---").join(" | ")} |\n`;
for (const n of names.sort())
  md += `| ${n} | ${ADAPTERS.map((a) => (unchecked(n, a) ? "not checked: reads DOM buttons, which synthetic drags leave at 0" : mark(results[a][n]))).join(" | ")} |\n`;
md += `\nControls in a plain textarea: dead key typed "${Object.values(results)[0]?.["control dead key"] ?? ""}", copy gave "${Object.values(results)[0]?.["control copy"] ?? ""}".\n`;
for (const a of ADAPTERS) {
  if (results[a].error) md += `\n${a} did not finish: ${results[a].error}.\n`;
  const errors = results[a]["page errors"] ?? [];
  if (errors.length)
    md += `\nErrors on the ${a} page:\n\n${errors.map((e) => "    " + e.split("\n").join("\n    ")).join("\n\n")}\n`;
}
const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
writeFileSync(join(here, "results", `checks-${stamp}.json`), JSON.stringify(results, null, 2));
writeFileSync(join(here, "results", `checks-${stamp}.md`), md);
writeFileSync(join(here, "results", "latest-checks.md"), md);
console.log(md);
process.exit(0);
